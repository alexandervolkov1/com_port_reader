use std::{
    fmt, fs,
    path::PathBuf,
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender, bounded};

use crate::{
    application_definition::ApplicationDefinition,
    lua_application_script::{LuaApplicationEvent, LuaControlInvocation},
    lua_runtime::LuaRuntime,
    user_command::UserCommand,
};

const COMMAND_CHANNEL_CAPACITY: usize = 32;

enum LuaCommand {
    Execute(String),
    InvokeControlCallback(LuaControlInvocation),
    Shutdown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LuaEvent {
    InitializationSucceeded,
    InitializationFailed(String),
    ExecutionSucceeded(Vec<String>),
    ExecutionFailed(String),
}

#[derive(Clone)]
pub struct LuaWorkerHandle {
    sender: Sender<LuaCommand>,
}

impl LuaWorkerHandle {
    pub fn execute(&self, source: impl Into<String>) -> Result<(), LuaWorkerHandleError> {
        self.send(LuaCommand::Execute(source.into()))
    }

    pub(crate) fn invoke_control_callback(
        &self,
        invocation: LuaControlInvocation,
    ) -> Result<(), LuaWorkerHandleError> {
        self.send(LuaCommand::InvokeControlCallback(invocation))
    }

    fn shutdown(&self) -> Result<(), LuaWorkerHandleError> {
        self.send(LuaCommand::Shutdown)
    }

    fn send(&self, command: LuaCommand) -> Result<(), LuaWorkerHandleError> {
        self.sender.send(command).map_err(|_| LuaWorkerHandleError)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LuaWorkerHandleError;

impl fmt::Display for LuaWorkerHandleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Lua worker command channel is disconnected")
    }
}

impl std::error::Error for LuaWorkerHandleError {}

struct ApplicationScriptSource {
    path: PathBuf,
    source: String,
}

pub struct LuaWorker {
    thread: Option<JoinHandle<()>>,
    commands: LuaWorkerHandle,
}

impl LuaWorker {
    pub(crate) fn spawn(
        event_sender: Sender<LuaEvent>,
        application_command_sender: Sender<UserCommand>,
        application_event_sender: Sender<LuaApplicationEvent>,
        application_definition: ApplicationDefinition,
        startup_source: Option<String>,
        application_script_paths: Vec<PathBuf>,
    ) -> std::io::Result<Self> {
        let (command_sender, command_receiver) = bounded(COMMAND_CHANNEL_CAPACITY);

        let commands = LuaWorkerHandle {
            sender: command_sender,
        };

        let thread = thread::Builder::new()
            .name("lua-runtime".to_owned())
            .spawn(move || {
                run_lua_worker(
                    command_receiver,
                    event_sender,
                    application_command_sender,
                    application_event_sender,
                    application_definition,
                    startup_source,
                    application_script_paths,
                );
            })?;

        Ok(Self {
            thread: Some(thread),
            commands,
        })
    }

    pub fn handle(&self) -> LuaWorkerHandle {
        self.commands.clone()
    }
}

impl Drop for LuaWorker {
    fn drop(&mut self) {
        let _ = self.commands.shutdown();

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_lua_worker(
    command_receiver: Receiver<LuaCommand>,
    event_sender: Sender<LuaEvent>,
    application_command_sender: Sender<UserCommand>,
    application_event_sender: Sender<LuaApplicationEvent>,
    application_definition: ApplicationDefinition,
    startup_source: Option<String>,
    application_script_paths: Vec<PathBuf>,
) {
    let application_scripts = match load_application_scripts(application_script_paths) {
        Ok(scripts) => scripts,

        Err(error) => {
            let _ = event_sender.send(LuaEvent::InitializationFailed(error));

            return;
        }
    };

    let runtime = LuaRuntime::with_application_definition(application_definition);

    if let Err(error) = runtime
        .install_application_api(application_command_sender, application_event_sender.clone())
    {
        let _ = event_sender.send(LuaEvent::InitializationFailed(error.to_string()));

        return;
    }

    if let Some(source) = startup_source
        && let Err(error) = runtime.execute_startup(&source)
    {
        let _ = event_sender.send(LuaEvent::InitializationFailed(format!(
            "Lua setup failed: {error}",
        )));

        return;
    }

    for script in application_scripts {
        let script_name = script.path.to_string_lossy();

        if let Err(error) = runtime.execute_named(&script.source, &script_name) {
            let _ = event_sender.send(LuaEvent::InitializationFailed(format!(
                "Lua application script '{}' failed: \
                     {error}",
                script.path.display(),
            )));

            return;
        }
    }

    if event_sender
        .send(LuaEvent::InitializationSucceeded)
        .is_err()
    {
        return;
    }

    while let Ok(command) = command_receiver.recv() {
        match command {
            LuaCommand::Execute(source) => {
                let event = match runtime.evaluate_for_repl(&source) {
                    Ok(output) => LuaEvent::ExecutionSucceeded(output),

                    Err(error) => LuaEvent::ExecutionFailed(error.to_string()),
                };

                if event_sender.send(event).is_err() {
                    break;
                }
            }

            LuaCommand::InvokeControlCallback(invocation) => {
                let event = match runtime.invoke_control_callback(&invocation) {
                    Ok(()) => LuaApplicationEvent::ControlCallbackSucceeded { invocation },

                    Err(error) => LuaApplicationEvent::ControlCallbackFailed {
                        invocation,
                        error: error.to_string(),
                    },
                };

                if application_event_sender.send(event).is_err() {
                    break;
                }
            }

            LuaCommand::Shutdown => break,
        }
    }
}

fn load_application_scripts(paths: Vec<PathBuf>) -> Result<Vec<ApplicationScriptSource>, String> {
    paths
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(&path).map_err(|error| {
                format!(
                    "Failed to read Lua \
                             application script '{}': \
                             {error}",
                    path.display(),
                )
            })?;

            Ok(ApplicationScriptSource { path, source })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crossbeam_channel::{Receiver, Sender, unbounded};

    use super::{LuaEvent, LuaWorker, load_application_scripts};

    use crate::{application_definition::ApplicationDefinition, user_command::UserCommand};

    const TEST_TIMEOUT: Duration = Duration::from_secs(2);

    fn spawn_worker(
        event_sender: Sender<LuaEvent>,
        event_receiver: &Receiver<LuaEvent>,
    ) -> LuaWorker {
        let (application_command_sender, _application_command_receiver) = unbounded();

        let (application_event_sender, _) = unbounded();

        let worker = LuaWorker::spawn(
            event_sender,
            application_command_sender,
            application_event_sender,
            ApplicationDefinition::default(),
            None,
            Vec::new(),
        )
        .unwrap();

        assert_initialized(event_receiver);

        worker
    }

    fn receive_event(receiver: &Receiver<LuaEvent>) -> LuaEvent {
        receiver.recv_timeout(TEST_TIMEOUT).unwrap()
    }

    fn assert_initialized(receiver: &Receiver<LuaEvent>) {
        assert_eq!(receive_event(receiver), LuaEvent::InitializationSucceeded,);
    }

    #[test]
    fn executes_code_on_worker_thread() {
        let (event_sender, event_receiver) = unbounded();

        let worker = spawn_worker(event_sender, &event_receiver);

        worker.handle().execute("value = 42").unwrap();

        assert_eq!(
            receive_event(&event_receiver),
            LuaEvent::ExecutionSucceeded(Vec::new(),),
        );
    }

    #[test]
    fn preserves_state_between_commands() {
        let (event_sender, event_receiver) = unbounded();

        let worker = spawn_worker(event_sender, &event_receiver);

        let handle = worker.handle();

        handle.execute("counter = 40").unwrap();

        assert_eq!(
            receive_event(&event_receiver),
            LuaEvent::ExecutionSucceeded(Vec::new(),),
        );

        handle.execute("counter + 2").unwrap();

        assert_eq!(
            receive_event(&event_receiver),
            LuaEvent::ExecutionSucceeded(vec!["42".to_owned(),]),
        );
    }

    #[test]
    fn returns_multiple_values() {
        let (event_sender, event_receiver) = unbounded();

        let worker = spawn_worker(event_sender, &event_receiver);

        worker.handle().execute("return 42, true, 'hello'").unwrap();

        assert_eq!(
            receive_event(&event_receiver),
            LuaEvent::ExecutionSucceeded(vec![
                "42".to_owned(),
                "true".to_owned(),
                "hello".to_owned(),
            ]),
        );
    }

    #[test]
    fn reports_execution_error() {
        let (event_sender, event_receiver) = unbounded();

        let worker = spawn_worker(event_sender, &event_receiver);

        worker.handle().execute("error('test failure')").unwrap();

        let event = receive_event(&event_receiver);

        let LuaEvent::ExecutionFailed(error) = event else {
            panic!("expected Lua execution failure");
        };

        assert!(error.contains("test failure"));
    }

    #[test]
    fn forwards_application_command() {
        let (event_sender, event_receiver) = unbounded();

        let (application_command_sender, application_command_receiver) = unbounded();

        let (application_event_sender, _) = unbounded();
        let worker = LuaWorker::spawn(
            event_sender,
            application_command_sender,
            application_event_sender,
            ApplicationDefinition::default(),
            None,
            Vec::new(),
        )
        .unwrap();

        assert_initialized(&event_receiver);

        worker.handle().execute("app.start()").unwrap();

        assert!(matches!(
            application_command_receiver
                .recv_timeout(TEST_TIMEOUT)
                .unwrap(),
            UserCommand::Start,
        ));

        assert_eq!(
            receive_event(&event_receiver),
            LuaEvent::ExecutionSucceeded(Vec::new(),),
        );
    }

    #[test]
    fn loads_application_script_sources() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let path = std::env::temp_dir().join(format!(
            "com_port_reader_application_script_{}_{}.lua",
            std::process::id(),
            unique,
        ));

        std::fs::write(&path, "application_script_value = 42").unwrap();

        let scripts = load_application_scripts(vec![path.clone()]).unwrap();

        let _ = std::fs::remove_file(&path);

        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0].path, path);

        assert_eq!(scripts[0].source, "application_script_value = 42",);
    }
}
