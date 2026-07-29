use std::{
    fmt,
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender, bounded};

use crate::lua_runtime::LuaRuntime;

const COMMAND_CHANNEL_CAPACITY: usize = 32;

enum LuaCommand {
    Execute(String),
    Shutdown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LuaEvent {
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

pub struct LuaWorker {
    thread: Option<JoinHandle<()>>,
    commands: LuaWorkerHandle,
}

impl LuaWorker {
    pub fn spawn(event_sender: Sender<LuaEvent>) -> std::io::Result<Self> {
        let (command_sender, command_receiver) = bounded(COMMAND_CHANNEL_CAPACITY);

        let commands = LuaWorkerHandle {
            sender: command_sender,
        };

        let thread = thread::Builder::new()
            .name("lua-runtime".to_owned())
            .spawn(move || {
                run_lua_worker(command_receiver, event_sender);
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

fn run_lua_worker(command_receiver: Receiver<LuaCommand>, event_sender: Sender<LuaEvent>) {
    let runtime = LuaRuntime::new();

    while let Ok(command) = command_receiver.recv() {
        let event = match command {
            LuaCommand::Execute(source) => match runtime.evaluate_for_repl(&source) {
                Ok(output) => LuaEvent::ExecutionSucceeded(output),

                Err(error) => LuaEvent::ExecutionFailed(error.to_string()),
            },

            LuaCommand::Shutdown => break,
        };

        if event_sender.send(event).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crossbeam_channel::unbounded;

    use super::{LuaEvent, LuaWorker};

    const TEST_TIMEOUT: Duration = Duration::from_secs(2);

    #[test]
    fn executes_code_on_worker_thread() {
        let (event_sender, event_receiver) = unbounded();

        let worker = LuaWorker::spawn(event_sender).unwrap();

        worker.handle().execute("value = 42").unwrap();

        let event = event_receiver.recv_timeout(TEST_TIMEOUT).unwrap();

        assert_eq!(event, LuaEvent::ExecutionSucceeded(Vec::new(),),);
    }

    #[test]
    fn preserves_state_between_commands() {
        let (event_sender, event_receiver) = unbounded();

        let worker = LuaWorker::spawn(event_sender).unwrap();

        let handle = worker.handle();

        handle.execute("counter = 40").unwrap();

        assert_eq!(
            event_receiver.recv_timeout(TEST_TIMEOUT).unwrap(),
            LuaEvent::ExecutionSucceeded(Vec::new(),),
        );

        handle.execute("counter + 2").unwrap();

        assert_eq!(
            event_receiver.recv_timeout(TEST_TIMEOUT).unwrap(),
            LuaEvent::ExecutionSucceeded(vec!["42".to_owned(),]),
        );
    }

    #[test]
    fn returns_multiple_values() {
        let (event_sender, event_receiver) = unbounded();

        let worker = LuaWorker::spawn(event_sender).unwrap();

        worker.handle().execute("return 42, true, 'hello'").unwrap();

        assert_eq!(
            event_receiver.recv_timeout(TEST_TIMEOUT).unwrap(),
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

        let worker = LuaWorker::spawn(event_sender).unwrap();

        worker.handle().execute("error('test failure')").unwrap();

        let event = event_receiver.recv_timeout(TEST_TIMEOUT).unwrap();

        let LuaEvent::ExecutionFailed(error) = event else {
            panic!("expected Lua execution failure");
        };

        assert!(error.contains("test failure"));
    }
}
