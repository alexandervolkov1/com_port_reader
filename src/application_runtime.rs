use crossbeam_channel::{Receiver, RecvTimeoutError, unbounded};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime},
};

use crate::{
    app_log::LogHandle,
    application_definition::ApplicationDefinition,
    application_paths::ApplicationPaths,
    connection::ConnectionId,
    data::{Sample, SeriesId, SeriesSample, SeriesStore},
    lua_application_definition::apply_lua_definition,
    lua_application_script::{LuaApplicationEvent, LuaControlInvocation},
    lua_worker::{LuaEvent, LuaWorker, LuaWorkerHandle, LuaWorkerHandleError},
    process_recorder::{ProcessAction, ProcessActionOrigin, ProcessRecord, ProcessRecorder},
    serial_connection::SerialConnectionRegistry,
    signal_processing::{SignalProcessingEvent, SignalProcessingService},
    user_command::UserCommand,
    worker::{ConnectionWorkers, WorkerConfig, spawn_serial_connection_worker},
};

mod acquisition_controller;
mod command_dispatcher;
mod device_emulator_service;

pub(crate) use acquisition_controller::AcquisitionController;
pub(crate) use command_dispatcher::CommandDispatcher;
pub(crate) use device_emulator_service::DeviceEmulatorService;

const LUA_INITIALIZATION_TIMEOUT: Duration = Duration::from_secs(10);
const LUA_INITIALIZATION_POLL_INTERVAL: Duration = Duration::from_millis(10);
const RUNTIME_STOP_TIMEOUT: Duration = Duration::from_secs(10);
const RUNTIME_STOP_POLL_INTERVAL: Duration = Duration::from_millis(10);

pub struct ApplicationRuntime {
    lua_worker: LuaWorker,
    definition: ApplicationDefinition,
    paths: ApplicationPaths,
    log: LogHandle,
    process_recorder: ProcessRecorder,
    series: SeriesStore,
    acquisition: AcquisitionController,
    signal_processing: SignalProcessingService<SeriesId>,
    dispatcher: CommandDispatcher,
    device_emulator: DeviceEmulatorService,
    lua_command_receiver: Receiver<UserCommand>,
    lua_application_event_receiver: Receiver<LuaApplicationEvent>,
}

impl ApplicationRuntime {
    pub(crate) fn build(
        definition: ApplicationDefinition,
        log: LogHandle,
        process_recorder: ProcessRecorder,
        paths: ApplicationPaths,
        startup_source: Option<String>,
    ) -> std::io::Result<(Self, Receiver<LuaEvent>)> {
        let (lua_event_sender, lua_event_receiver) = unbounded();

        let (lua_command_sender, lua_command_receiver) = unbounded();

        let (lua_application_event_sender, lua_application_event_receiver) = unbounded();

        let configuration_source = startup_source.clone();

        let application_script_paths = definition
            .scripts()
            .iter()
            .map(|script| paths.resolve_profile(script.path()))
            .collect::<Vec<_>>();

        let lua_worker = LuaWorker::spawn(
            lua_event_sender,
            lua_command_sender,
            lua_application_event_sender,
            definition.clone(),
            startup_source,
            application_script_paths,
        )?;

        let series = SeriesStore::new();

        let signal_processing = SignalProcessingService::<SeriesId>::spawn()?;

        let signal_processing_handle = signal_processing.handle();

        let emulator_port = definition
            .emulator()
            .map(|emulator| emulator.port_name().to_owned());

        let emulator_script_path = definition
            .emulator()
            .map(|emulator| paths.resolve_profile(emulator.script_path()));

        let device_emulator =
            DeviceEmulatorService::new(emulator_port, emulator_script_path, log.clone());

        let (event_sender, event_receiver) = crossbeam_channel::unbounded();

        let serial_connections = SerialConnectionRegistry::new();

        for connection in definition.serial_connections() {
            let connection_id = connection.id();

            let store = if connection_id == ConnectionId::PRIMARY {
                serial_connections.primary()
            } else {
                serial_connections.register(connection_id).expect(
                    "validated application \
                             definition must contain \
                             unique connection IDs",
                )
            };

            store.set(Some(connection.serial_config().clone()));
        }

        let worker_config = WorkerConfig::new(definition.runtime().default_poll_interval());

        let primary_worker = spawn_serial_connection_worker(
            serial_connections.primary(),
            event_sender.clone(),
            series.clone(),
            process_recorder.clone(),
            signal_processing_handle.clone(),
            worker_config,
        );

        let mut workers = ConnectionWorkers::new(primary_worker);

        for connection in definition.serial_connections() {
            let connection_id = connection.id();

            if connection_id == ConnectionId::PRIMARY {
                continue;
            }

            let config_store = serial_connections.store(connection_id).expect(
                "serial connection store was \
                     registered before spawning its \
                     worker",
            );

            let worker = spawn_serial_connection_worker(
                config_store,
                event_sender.clone(),
                series.clone(),
                process_recorder.clone(),
                signal_processing_handle.clone(),
                worker_config,
            );

            workers.insert(worker).expect(
                "validated application definition must \
                 contain unique connection IDs",
            );
        }

        let connection_router = workers.router();

        let acquisition = AcquisitionController::new(workers, log.clone());

        let dispatcher = CommandDispatcher::new(
            connection_router,
            serial_connections,
            definition.clone(),
            series.clone(),
            event_receiver,
            log.clone(),
        );

        process_recorder.record(ProcessRecord::ConfigurationLoaded {
            timestamp: SystemTime::now(),
            startup_path: paths.startup_script().to_path_buf(),
            source: configuration_source,
        });

        let runtime = Self::new(
            lua_worker,
            definition,
            paths,
            log,
            process_recorder,
            series,
            acquisition,
            signal_processing,
            dispatcher,
            device_emulator,
            lua_command_receiver,
            lua_application_event_receiver,
        );

        Ok((runtime, lua_event_receiver))
    }

    fn build_initialized(
        definition: ApplicationDefinition,
        log: LogHandle,
        process_recorder: ProcessRecorder,
        paths: ApplicationPaths,
        startup_source: Option<String>,
    ) -> Result<(Self, Receiver<LuaEvent>), String> {
        let (mut runtime, lua_event_receiver) =
            Self::build(definition, log, process_recorder, paths, startup_source)
                .map_err(|error| format!("Failed to spawn application runtime: {error}"))?;

        runtime.wait_for_lua_initialization(&lua_event_receiver)?;

        Ok((runtime, lua_event_receiver))
    }

    fn wait_for_lua_initialization(
        &mut self,
        lua_event_receiver: &Receiver<LuaEvent>,
    ) -> Result<(), String> {
        let deadline = Instant::now() + LUA_INITIALIZATION_TIMEOUT;

        loop {
            self.poll();

            let now = Instant::now();

            if now >= deadline {
                return Err(format!(
                    "Lua runtime initialization timed out after {:.1} s",
                    LUA_INITIALIZATION_TIMEOUT.as_secs_f64(),
                ));
            }

            let timeout = (deadline - now).min(LUA_INITIALIZATION_POLL_INTERVAL);

            match lua_event_receiver.recv_timeout(timeout) {
                Ok(LuaEvent::InitializationSucceeded) => {
                    return Ok(());
                }

                Ok(LuaEvent::InitializationFailed(error)) => {
                    return Err(error);
                }

                Ok(LuaEvent::ExecutionSucceeded(_)) | Ok(LuaEvent::ExecutionFailed(_)) => {
                    return Err("Lua runtime produced an execution event \
                         before initialization completed"
                        .to_owned());
                }

                Err(RecvTimeoutError::Timeout) => {}

                Err(RecvTimeoutError::Disconnected) => {
                    return Err("Lua worker disconnected during initialization".to_owned());
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        lua_worker: LuaWorker,
        definition: ApplicationDefinition,
        paths: ApplicationPaths,
        log: LogHandle,
        process_recorder: ProcessRecorder,
        series: SeriesStore,
        acquisition: AcquisitionController,
        signal_processing: SignalProcessingService<SeriesId>,
        dispatcher: CommandDispatcher,
        device_emulator: DeviceEmulatorService,
        lua_command_receiver: Receiver<UserCommand>,
        lua_application_event_receiver: Receiver<LuaApplicationEvent>,
    ) -> Self {
        Self {
            lua_worker,
            definition,
            paths,
            log,
            process_recorder,
            series,
            acquisition,
            signal_processing,
            dispatcher,
            device_emulator,
            lua_command_receiver,
            lua_application_event_receiver,
        }
    }

    pub fn poll(&mut self) {
        if let Some(error) = self.process_recorder.take_error() {
            self.log.error(format!("Process recorder failed: {error}",));
        }

        self.device_emulator.poll();

        self.dispatcher.poll_events();

        self.poll_signal_processing();

        let commands = self.lua_command_receiver.try_iter().collect::<Vec<_>>();

        for command in commands {
            self.execute_from(command, ProcessActionOrigin::Lua);
        }
    }

    pub fn execute(&mut self, command: UserCommand) {
        self.execute_from(command, ProcessActionOrigin::UserInterface);
    }

    fn execute_from(&mut self, command: UserCommand, origin: ProcessActionOrigin) {
        if let Some(action) = process_action_from_command(&command) {
            self.process_recorder.record_action(origin, action);
        }

        self.dispatcher
            .execute(command, &mut self.acquisition, &mut self.device_emulator);
    }

    pub fn is_running(&self) -> bool {
        self.acquisition.is_running()
    }

    pub fn set_series_visibility(&self, id: SeriesId, visible: bool) {
        self.process_recorder.record_action(
            ProcessActionOrigin::UserInterface,
            ProcessAction::SetSeriesVisibility {
                series_id: id,
                visible,
            },
        );

        self.dispatcher.set_visibility(id, visible);
    }

    pub(crate) fn lua_handle(&self) -> LuaWorkerHandle {
        self.lua_worker.handle()
    }

    pub(crate) fn invoke_control_callback(
        &self,
        invocation: LuaControlInvocation,
    ) -> Result<(), LuaWorkerHandleError> {
        self.lua_worker.handle().invoke_control_callback(invocation)
    }

    pub(crate) fn log_error(&self, message: impl Into<String>) {
        self.log.error(message);
    }

    pub(crate) const fn series(&self) -> &SeriesStore {
        &self.series
    }

    pub(crate) const fn definition(&self) -> &ApplicationDefinition {
        &self.definition
    }

    pub(crate) const fn paths(&self) -> &ApplicationPaths {
        &self.paths
    }

    pub(crate) fn validate_startup_configuration(&self) -> Result<(), String> {
        Self::load_startup_configuration(&self.paths).map(|_| ())
    }

    pub(crate) fn validate_profile_configuration(
        &self,
        startup_script: &Path,
    ) -> Result<(), String> {
        let paths = self
            .paths
            .with_startup_script(startup_script)
            .map_err(|error| error.to_string())?;

        Self::load_startup_configuration(&paths).map(|_| ())
    }

    pub(crate) fn open_startup_configuration(&self) -> Result<(), String> {
        let path = self.paths.startup_script();

        if !path.is_file() {
            return Err(format!("Startup file '{}' does not exist", path.display(),));
        }

        open::that(path).map_err(|error| {
            format!(
                "Failed to open startup file '{}': \
                 {error}",
                path.display(),
            )
        })
    }

    fn stop_active_operations(&mut self) -> Result<(), String> {
        let has_active_operations = self.is_running() || self.device_emulator.is_running();

        if !has_active_operations {
            return Ok(());
        }

        self.log.info(
            "Stopping the active runtime before loading \
             an application profile.",
        );

        let deadline = Instant::now() + RUNTIME_STOP_TIMEOUT;

        if self.is_running() {
            self.execute(UserCommand::Stop);
        }

        if self.device_emulator.is_running() {
            self.execute(UserCommand::StopEmulator);
        }

        loop {
            self.poll();

            if !self.is_running() && !self.device_emulator.is_running() {
                break;
            }

            Self::wait_for_stop_progress(deadline)?;
        }

        self.log
            .info("Active runtime stopped before profile loading.");

        Ok(())
    }

    fn wait_for_stop_progress(deadline: Instant) -> Result<(), String> {
        let now = Instant::now();

        if now >= deadline {
            return Err(format!(
                "Timed out after {:.1} s while stopping \
                 the active runtime",
                RUNTIME_STOP_TIMEOUT.as_secs_f64(),
            ));
        }

        thread::sleep((deadline - now).min(RUNTIME_STOP_POLL_INTERVAL));

        Ok(())
    }

    pub(crate) fn rebuild_from_startup(&mut self) -> Result<(Self, Receiver<LuaEvent>), String> {
        let paths = self.paths.clone();

        self.rebuild_from_paths(paths)
    }

    pub(crate) fn rebuild_from_profile(
        &mut self,
        startup_script: &Path,
    ) -> Result<(Self, Receiver<LuaEvent>), String> {
        let paths = self
            .paths
            .with_startup_script(startup_script)
            .map_err(|error| {
                let message = format!(
                    "Failed to select Lua profile '{}': {error}",
                    startup_script.display(),
                );

                self.log.error(message.clone());

                message
            })?;

        self.rebuild_from_paths(paths)
    }

    fn rebuild_from_paths(
        &mut self,
        paths: ApplicationPaths,
    ) -> Result<(Self, Receiver<LuaEvent>), String> {
        let startup_path = paths.startup_script().to_path_buf();

        let result = self
            .stop_active_operations()
            .and_then(|()| self.try_rebuild_from_paths(paths));

        match &result {
            Ok(_) => {
                self.log.info(format!(
                    "Application profile loaded from '{}'.",
                    startup_path.display(),
                ));
            }

            Err(error) => {
                self.log.error(format!(
                    "Failed to load application profile '{}': {error}",
                    startup_path.display(),
                ));
            }
        }

        result
    }

    fn try_rebuild_from_paths(
        &self,
        paths: ApplicationPaths,
    ) -> Result<(Self, Receiver<LuaEvent>), String> {
        let (definition, source) = Self::load_startup_configuration(&paths)?;

        Self::build_initialized(
            definition,
            self.log.clone(),
            self.process_recorder.clone(),
            paths,
            Some(source),
        )
    }

    fn load_startup_configuration(
        paths: &ApplicationPaths,
    ) -> Result<(ApplicationDefinition, String), String> {
        let path = paths.startup_script();

        let source = fs::read_to_string(path).map_err(|error| {
            format!("Failed to read startup file '{}': {error}", path.display(),)
        })?;

        let definition =
            apply_lua_definition(&source, &ApplicationDefinition::default()).map_err(|error| {
                format!(
                    "Failed to validate startup file '{}': {error}",
                    path.display(),
                )
            })?;

        Ok((definition, source))
    }

    pub(crate) fn take_lua_application_events(&self) -> Vec<LuaApplicationEvent> {
        self.lua_application_event_receiver.try_iter().collect()
    }

    fn poll_signal_processing(&self) {
        for event in self.signal_processing.take_events() {
            match event {
                SignalProcessingEvent::Samples(samples) => {
                    self.store_processed_samples(samples);
                }

                SignalProcessingEvent::Error(error) => {
                    self.log.error(error.to_string());
                }
            }
        }
    }

    fn store_processed_samples(
        &self,
        samples: Vec<crate::signal_processing::ProcessedSignal<SeriesId>>,
    ) {
        if samples.is_empty() {
            return;
        }

        let series_samples = samples
            .into_iter()
            .map(|processed| {
                SeriesSample::new(
                    processed.signal_id,
                    Sample::new(processed.timestamp, processed.value),
                )
            })
            .collect::<Vec<_>>();

        if let Err(error) = self.series.append_samples(&series_samples) {
            self.log
                .error(format!("Failed to store processed signal: {error}",));

            return;
        }

        let metadata = self.series.metadata();

        let mut samples_by_connection: BTreeMap<ConnectionId, Vec<SeriesSample>> = BTreeMap::new();

        for series_sample in series_samples {
            let Some(series_metadata) = metadata
                .iter()
                .find(|metadata| metadata.id == series_sample.series_id)
            else {
                self.log.error(format!(
                    "Processed series {} disappeared \
                     before it could be recorded",
                    series_sample.series_id,
                ));

                continue;
            };

            samples_by_connection
                .entry(series_metadata.connection_id)
                .or_default()
                .push(series_sample);
        }

        for (connection_id, samples) in samples_by_connection {
            self.process_recorder
                .record_measurements(connection_id, &samples, &metadata);
        }
    }
}

fn process_action_from_command(command: &UserCommand) -> Option<ProcessAction> {
    match command {
        UserCommand::Add(new_series) => Some(ProcessAction::AddSeries {
            connection_id: new_series.connection_id(),

            name: new_series.name().map(str::to_owned),

            source: new_series.source().to_string(),

            polling_interval_seconds: new_series
                .sampling_interval()
                .map(|interval| interval.duration().as_secs_f64()),

            color: new_series.color().map(|color| color.to_string()),
        }),

        UserCommand::AddFilter(filter) => Some(ProcessAction::AddFilteredSeries {
            input_name: filter.input_name().to_owned(),
            name: filter.name().to_owned(),
            definition: filter.definition().to_string(),
            color: filter.color().map(|color| color.to_string()),
        }),

        UserCommand::SetFilter { name, definition } => Some(ProcessAction::SetFilter {
            name: name.clone(),
            definition: definition.to_string(),
        }),

        UserCommand::Delete { name } => {
            Some(ProcessAction::DeleteSeriesByName { name: name.clone() })
        }

        UserCommand::Rename {
            current_name,
            new_name,
        } => Some(ProcessAction::RenameSeries {
            current_name: current_name.clone(),
            new_name: new_name.clone(),
        }),

        UserCommand::SetSeriesColor { name, color } => Some(ProcessAction::SetSeriesColor {
            name: name.clone(),
            color: color.map(|color| color.to_string()),
        }),

        UserCommand::Retry { .. } | UserCommand::RetryAll | UserCommand::Log { .. } => None,

        UserCommand::Start => Some(ProcessAction::StartAcquisition),

        UserCommand::Stop => Some(ProcessAction::StopAcquisition),

        UserCommand::Clear => Some(ProcessAction::ClearSeries),

        UserCommand::StartEmulator => Some(ProcessAction::StartEmulator),

        UserCommand::StopEmulator => Some(ProcessAction::StopEmulator),

        UserCommand::SendSerial {
            connection_id,
            command,
        } => Some(ProcessAction::SendSerial {
            connection_id: *connection_id,
            command: command.clone(),
        }),

        UserCommand::ReadInstrument {
            connection_id,
            request,
            ..
        } => Some(ProcessAction::ReadInstrument {
            connection_id: *connection_id,
            request: request.to_string(),
        }),

        UserCommand::WriteInstrument {
            connection_id,
            request,
            ..
        } => Some(ProcessAction::WriteInstrument {
            connection_id: *connection_id,
            request: request.to_string(),
        }),

        UserCommand::DescribeVirtualInstruments { connection_id, .. } => {
            Some(ProcessAction::DescribeVirtualInstruments {
                connection_id: *connection_id,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{ProcessAction, process_action_from_command};

    use crate::{
        connection::ConnectionId,
        data::{NewSeries, SamplingInterval, SeriesColor},
        signal_processing::SignalFilterDefinition,
        user_command::UserCommand,
    };

    #[test]
    fn converts_added_series_to_process_action() {
        let interval = SamplingInterval::new(Duration::from_millis(250)).unwrap();

        let color = SeriesColor::new(0x1A, 0x2B, 0x3C);

        let command = UserCommand::Add(
            NewSeries::named_serial_command("read temperature", "temperature")
                .with_connection(ConnectionId::new(2))
                .with_sampling_interval(interval)
                .with_color(color),
        );

        assert_eq!(
            process_action_from_command(&command),
            Some(ProcessAction::AddSeries {
                connection_id: ConnectionId::new(2),
                name: Some("temperature".to_owned()),
                source: "COM command: read temperature".to_owned(),
                polling_interval_seconds: Some(0.25),
                color: Some("#1A2B3C".to_owned()),
            }),
        );
    }

    #[test]
    fn converts_series_color_change_to_process_action() {
        let command = UserCommand::SetSeriesColor {
            name: "temperature".to_owned(),
            color: Some(SeriesColor::new(0x1A, 0x2B, 0x3C)),
        };

        assert_eq!(
            process_action_from_command(&command),
            Some(ProcessAction::SetSeriesColor {
                name: "temperature".to_owned(),
                color: Some("#1A2B3C".to_owned()),
            }),
        );
    }

    #[test]
    fn does_not_duplicate_log_as_action() {
        let command = UserCommand::Log {
            message: "test".to_owned(),
        };

        assert_eq!(process_action_from_command(&command), None,);
    }

    #[test]
    fn converts_filter_change_to_process_action() {
        let definition = SignalFilterDefinition::median(7).unwrap();

        let command = UserCommand::SetFilter {
            name: "temperature_filtered".to_owned(),
            definition,
        };

        assert_eq!(
            process_action_from_command(&command),
            Some(ProcessAction::SetFilter {
                name: "temperature_filtered".to_owned(),
                definition: definition.to_string(),
            }),
        );
    }
}
