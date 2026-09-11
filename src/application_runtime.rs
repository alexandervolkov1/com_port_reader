use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime},
};

use crossbeam_channel::{Receiver, RecvTimeoutError, unbounded};

use crate::{
    app_log::LogHandle,
    application_definition::ApplicationDefinition,
    application_paths::ApplicationPaths,
    connection::ConnectionId,
    data::{Sample, SeriesId, SeriesSample, SeriesStore},
    lua_application_definition::apply_lua_definition,
    lua_application_script::{LuaApplicationEvent, LuaControlInvocation},
    lua_worker::{LuaEvent, LuaWorker, LuaWorkerHandle, LuaWorkerHandleError},
    output_control::{OutputHandle, OutputService},
    process_recorder::{
        ProcessAction, ProcessActionContext, ProcessActionOrigin, ProcessRecord, ProcessRecorder,
    },
    scenario::ScenarioService,
    serial_connection::SerialConnectionRegistry,
    signal_processing::{ProcessingEvent, ProcessingService},
    user_command::{AcquisitionCommand, EmulatorCommand, UserCommand},
    worker::{ConnectionWorkers, WorkerConfig, spawn_serial_connection_worker},
};

mod acquisition_command_handler;
mod acquisition_controller;
mod command_dispatcher;
mod controller_command_handler;
mod device_emulator_service;
mod emulator_command_handler;
mod instrument_command_handler;
mod process_action;
mod process_control_dispatcher;
mod serial_command_handler;
mod series_command_handler;

pub(crate) use acquisition_controller::AcquisitionController;
pub(crate) use command_dispatcher::{CommandDispatcher, CommandDispatcherConnections};
pub(crate) use device_emulator_service::DeviceEmulatorService;

use self::{
    process_action::{process_action_from_command, resolve_action_series_id},
    process_control_dispatcher::ProcessControlDispatcher,
};

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
    processing: ProcessingService<SeriesId>,
    _process_control_dispatcher: ProcessControlDispatcher,
    _output_service: OutputService,
    _output_handle: OutputHandle,
    dispatcher: CommandDispatcher,
    device_emulator: DeviceEmulatorService,
    lua_command_receiver: Receiver<UserCommand>,
    lua_application_event_receiver: Receiver<LuaApplicationEvent>,
    scenario: ScenarioService,
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

        let processing = ProcessingService::<SeriesId>::spawn()?;

        let processing_handle = processing.handle();

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
            processing_handle.clone(),
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
                processing_handle.clone(),
                worker_config,
            );

            workers.insert(worker).expect(
                "validated application definition must \
                 contain unique connection IDs",
            );
        }

        let connection_router = workers.router();

        let output_service =
            OutputService::spawn(connection_router.clone(), serial_connections.clone())?;

        let output_handle = output_service.handle();

        let process_control_dispatcher = ProcessControlDispatcher::spawn(
            processing.control_event_receiver(),
            output_handle.clone(),
            process_recorder.clone(),
            log.clone(),
        )?;

        let acquisition = AcquisitionController::new(workers);

        let dispatcher = CommandDispatcher::new(
            CommandDispatcherConnections::new(
                connection_router,
                serial_connections,
                event_receiver,
            ),
            definition.clone(),
            series.clone(),
            processing_handle,
            output_handle.clone(),
            process_recorder.clone(),
            log.clone(),
        );

        let scenario = ScenarioService::new(
            process_recorder.application_events().subscribe(),
            lua_worker.handle(),
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
            processing,
            process_control_dispatcher,
            output_service,
            output_handle,
            dispatcher,
            device_emulator,
            lua_command_receiver,
            lua_application_event_receiver,
            scenario,
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
        processing: ProcessingService<SeriesId>,
        process_control_dispatcher: ProcessControlDispatcher,
        output_service: OutputService,
        output_handle: OutputHandle,
        dispatcher: CommandDispatcher,
        device_emulator: DeviceEmulatorService,
        lua_command_receiver: Receiver<UserCommand>,
        lua_application_event_receiver: Receiver<LuaApplicationEvent>,
        scenario: ScenarioService,
    ) -> Self {
        Self {
            lua_worker,
            definition,
            paths,
            log,
            process_recorder,
            series,
            acquisition,
            processing,
            _process_control_dispatcher: process_control_dispatcher,
            _output_service: output_service,
            _output_handle: output_handle,
            dispatcher,
            device_emulator,
            lua_command_receiver,
            lua_application_event_receiver,
            scenario,
        }
    }

    pub fn poll(&mut self) {
        if let Some(error) = self.process_recorder.take_error() {
            self.log.error(format!("Process recorder failed: {error}",));
        }

        self.device_emulator.poll();

        self.dispatcher.poll_events();

        self.poll_processing();

        let commands = self.lua_command_receiver.try_iter().collect::<Vec<_>>();

        for command in commands {
            self.execute_from(command, ProcessActionOrigin::Lua);
        }

        self.scenario.poll();
    }

    pub fn execute(&mut self, command: UserCommand) {
        self.execute_from(command, ProcessActionOrigin::UserInterface);
    }

    fn execute_from(&mut self, command: UserCommand, origin: ProcessActionOrigin) {
        let command = match command {
            UserCommand::Scenario(command) => {
                self.scenario.execute(command);
                return;
            }

            command => command,
        };

        let action_context = process_action_from_command(&command).map(|mut action| {
            resolve_action_series_id(&mut action, &self.series);

            let action_id = self.process_recorder.record_action(origin, action);

            ProcessActionContext::new(action_id)
        });

        self.dispatcher.execute(
            command,
            action_context,
            &mut self.acquisition,
            &mut self.device_emulator,
        );
    }

    pub fn is_running(&self) -> bool {
        self.acquisition.is_running()
    }

    pub fn set_series_visibility(&self, id: SeriesId, visible: bool) {
        let series_name = self
            .series
            .metadata()
            .into_iter()
            .find(|series| series.id == id)
            .map(|series| series.name);

        let action_id = self.process_recorder.record_action(
            ProcessActionOrigin::UserInterface,
            ProcessAction::SetSeriesVisibility {
                series_id: id,
                series_name: series_name.clone(),
                visible,
            },
        );

        if self.series.set_visibility(id, visible) {
            self.process_recorder
                .record_action_applied(action_id, Some(id), series_name);
        } else {
            self.process_recorder
                .record_action_failed(action_id, format!("Series {id} not found."));
        }
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
            self.execute(UserCommand::Acquisition(AcquisitionCommand::Stop));
        }

        if self.device_emulator.is_running() {
            self.execute(EmulatorCommand::Stop.into());
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

    fn poll_processing(&self) {
        for event in self.processing.take_events() {
            match event {
                ProcessingEvent::Samples(samples) => {
                    self.store_processed_samples(samples);
                }

                ProcessingEvent::Error(error) => {
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

        let metadata = self.series.metadata();

        let series_samples = samples
            .into_iter()
            .filter(|processed| {
                metadata
                    .iter()
                    .any(|series| series.id == processed.signal_id)
            })
            .map(|processed| {
                SeriesSample::new(
                    processed.signal_id,
                    Sample::new(processed.timestamp, processed.value),
                )
            })
            .collect::<Vec<_>>();

        if series_samples.is_empty() {
            return;
        }

        if let Err(error) = self.series.append_samples(&series_samples) {
            self.log
                .error(format!("Failed to store processed signal: {error}",));

            return;
        }

        let mut samples_by_connection: BTreeMap<ConnectionId, Vec<SeriesSample>> = BTreeMap::new();

        for series_sample in series_samples {
            let Some(series_metadata) = metadata
                .iter()
                .find(|metadata| metadata.id == series_sample.series_id)
            else {
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

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        thread,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use super::{
        ApplicationRuntime, ProcessAction, process_action_from_command, resolve_action_series_id,
    };
    use crate::{
        app_log::LogModel,
        application_paths::ApplicationPaths,
        connection::ConnectionId,
        data::{
            NewSeries, SamplingInterval, SeriesColor, SeriesId, SeriesPollingState, SeriesSource,
            SeriesStore,
        },
        instrument::{
            InstrumentValue, ParameterAccess, ParameterRange, ParameterValueType,
            virtual_instrument::{
                VirtualInstrumentId, VirtualParameterDescriptor, VirtualParameterId,
            },
        },
        lua_worker::LuaEvent,
        process_control::{
            ControlEvent, ControlLoopDefinition, ControlLoopState, ControlOutputTarget,
            ControllerKind, NewController, OnOffController, PidController, PidGains,
            PidOutputLimits, ReferenceKind, ReferenceSource,
        },
        process_recorder::ProcessRecorder,
        signal_processing::SignalFilterDefinition,
        user_command::{AcquisitionCommand, ControllerCommand, SeriesCommand, UserCommand},
    };

    fn runtime_test_directory(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir().join(format!(
            "com_port_reader_runtime_e2e_\
             {name}_{}_{}",
            std::process::id(),
            unique,
        ))
    }

    fn write_test_profile(directory: &Path, file_name: &str, source: &str) -> PathBuf {
        fs::create_dir_all(directory).unwrap();

        let path = directory.join(file_name);

        fs::write(&path, source).unwrap();

        path
    }

    fn test_profile_source(raw_name: &str, filtered_name: &str, port_name: &str) -> String {
        format!(
            r#"
    local definition = {{
        connections = {{
            primary = {{
                port = "{port_name}",
            }},
        }},
    }}

    function definition.setup()
        app.add_serial(
            "get",
            {{
                name = "{raw_name}",
                interval = 1.0,
            }}
        )

        app.filter(
            "{raw_name}",
            {{
                name = "{filtered_name}",
                kind = "moving_average",
                window = 3,
            }}
        )
    end

    return definition
    "#
        )
    }

    fn build_test_runtime(
        profile_path: &Path,
    ) -> (
        ApplicationRuntime,
        LogModel,
        crossbeam_channel::Receiver<LuaEvent>,
    ) {
        let paths = ApplicationPaths::from_startup_script(profile_path).unwrap();

        let log_directory = paths.resolve_data("logs");

        let (definition, source) = ApplicationRuntime::load_startup_configuration(&paths).unwrap();

        let recorder = ProcessRecorder::default();

        let (log_model, log) = LogModel::new(log_directory, recorder.clone());

        let (runtime, lua_events) =
            ApplicationRuntime::build_initialized(definition, log, recorder, paths, Some(source))
                .unwrap();

        (runtime, log_model, lua_events)
    }

    fn wait_for_series(runtime: &mut ApplicationRuntime, name: &str) {
        let deadline = Instant::now() + Duration::from_secs(2);

        loop {
            runtime.poll();

            if runtime.series().id_by_name(name).is_some() {
                return;
            }

            assert!(
                Instant::now() < deadline,
                "series '{name}' was not created \
                 before the E2E test timeout",
            );

            thread::sleep(Duration::from_millis(5));
        }
    }

    fn wait_for_polling_state(
        runtime: &mut ApplicationRuntime,
        log_model: &mut LogModel,
        name: &str,
        expected: SeriesPollingState,
    ) {
        let deadline = Instant::now() + Duration::from_secs(3);

        loop {
            runtime.poll();
            log_model.poll();

            let state = runtime
                .series()
                .metadata()
                .into_iter()
                .find(|series| series.name == name)
                .map(|series| series.polling_state)
                .unwrap_or_else(|| panic!("series '{name}' not found"));

            if state == expected {
                return;
            }

            assert!(
                Instant::now() < deadline,
                "series '{name}' did not reach \
                 polling state {expected:?}",
            );

            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn converts_added_series_to_process_action() {
        let interval = SamplingInterval::new(Duration::from_millis(250)).unwrap();

        let color = SeriesColor::new(0x1A, 0x2B, 0x3C);

        let command = SeriesCommand::Add(
            NewSeries::named_serial_command("read temperature", "temperature")
                .with_connection(ConnectionId::new(2))
                .with_sampling_interval(interval)
                .with_color(color),
        )
        .into();

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
        let command = SeriesCommand::SetColor {
            name: "temperature".to_owned(),
            color: Some(SeriesColor::new(0x1A, 0x2B, 0x3C)),
        }
        .into();

        assert_eq!(
            process_action_from_command(&command),
            Some(ProcessAction::SetSeriesColor {
                series_id: None,
                name: "temperature".to_owned(),
                color: Some("#1A2B3C".to_owned(),),
            },),
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

        let command = SeriesCommand::SetFilter {
            name: "temperature_filtered".to_owned(),
            definition,
        }
        .into();

        assert_eq!(
            process_action_from_command(&command),
            Some(ProcessAction::SetFilter {
                series_id: None,
                name: "temperature_filtered".to_owned(),
                definition: definition.to_string(),
            },),
        );
    }

    #[test]
    fn resolves_existing_series_id_for_action() {
        let series = SeriesStore::new();

        let series_id = series
            .add_series(NewSeries::named_serial_command(
                "read temperature",
                "temperature",
            ))
            .unwrap();

        let command = SeriesCommand::SetColor {
            name: "temperature".to_owned(),
            color: None,
        }
        .into();

        let mut action = process_action_from_command(&command).unwrap();

        resolve_action_series_id(&mut action, &series);

        assert_eq!(
            action,
            ProcessAction::SetSeriesColor {
                series_id: Some(series_id),
                name: "temperature".to_owned(),
                color: None,
            },
        );
    }

    #[test]
    fn keeps_missing_series_id_empty_for_action() {
        let series = SeriesStore::new();

        let command = SeriesCommand::Delete {
            name: "missing".to_owned(),
        }
        .into();

        let mut action = process_action_from_command(&command).unwrap();

        resolve_action_series_id(&mut action, &series);

        assert_eq!(
            action,
            ProcessAction::DeleteSeriesByName {
                series_id: None,
                name: "missing".to_owned(),
            },
        );
    }

    #[test]
    fn processing_service_runs_pid_for_raw_signal() {
        let processing = crate::signal_processing::ProcessingService::<SeriesId>::spawn().unwrap();

        let handle = processing.handle();

        let control_events = processing.control_event_receiver();

        let input = SeriesId::new(1);

        let descriptor = VirtualParameterDescriptor::new(
            VirtualParameterId::new(1),
            "heater_power",
            "Heater power",
            ParameterAccess::ReadWrite,
            ParameterValueType::Number,
        )
        .with_range(ParameterRange::Number {
            minimum: 0.0,
            maximum: 100.0,
        });

        let output = ControlOutputTarget::virtual_instrument(
            ConnectionId::PRIMARY,
            VirtualInstrumentId::new(1),
            &descriptor,
        )
        .unwrap();

        let controller = PidController::with_output_limits(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .unwrap()
        .into();

        let definition = ControlLoopDefinition::new("heater", input, output, controller).unwrap();

        handle.add_control_loop(definition).unwrap();

        handle.process(input, 1_000.0, 80.0).unwrap();

        let event = control_events.recv_timeout(Duration::from_secs(1)).unwrap();

        let ControlEvent::Output(output) = event else {
            panic!("expected PID output from processing service");
        };

        assert_eq!(output.loop_name, "heater");
        assert_eq!(output.input, input);
        assert_eq!(output.output.value(), 40.0);
    }

    #[test]
    fn records_controller_parameter_write_action() {
        let (response_sender, _response_receiver) = crossbeam_channel::bounded(1);

        let command: UserCommand = ControllerCommand::WriteParameter {
            name: "heater".to_owned(),
            key: "kd".to_owned(),
            value: InstrumentValue::Number(2.5),
            response_sender,
        }
        .into();

        assert_eq!(
            process_action_from_command(&command),
            Some(ProcessAction::WriteControllerParameter {
                name: "heater".to_owned(),
                key: "kd".to_owned(),
                value: InstrumentValue::Number(2.5),
            },),
        );
    }

    #[test]
    fn records_controller_configuration_action() {
        let (response_sender, _response_receiver) = crossbeam_channel::bounded(1);

        let updates = vec![
            ("kp".to_owned(), InstrumentValue::Number(1.5)),
            ("ki".to_owned(), InstrumentValue::Number(0.25)),
        ];

        let command: UserCommand = ControllerCommand::Configure {
            name: "heater".to_owned(),
            updates: updates.clone(),
            response_sender,
        }
        .into();

        assert_eq!(
            process_action_from_command(&command),
            Some(ProcessAction::ConfigureController {
                name: "heater".to_owned(),
                updates,
            },),
        );
    }

    #[test]
    fn records_controller_reference_parameter_write_action() {
        let (response_sender, _response_receiver) = crossbeam_channel::bounded(1);

        let command: UserCommand = ControllerCommand::WriteReferenceParameter {
            name: "heater".to_owned(),
            key: "target".to_owned(),
            value: InstrumentValue::Number(220.0),
            response_sender,
        }
        .into();

        assert_eq!(
            process_action_from_command(&command),
            Some(ProcessAction::WriteControllerReferenceParameter {
                name: "heater".to_owned(),
                key: "target".to_owned(),
                value: InstrumentValue::Number(220.0,),
            },),
        );
    }

    #[test]
    fn records_controller_reference_configuration_action() {
        let (response_sender, _response_receiver) = crossbeam_channel::bounded(1);

        let updates = vec![
            ("target".to_owned(), InstrumentValue::Number(220.0)),
            ("rate".to_owned(), InstrumentValue::Number(2.0)),
        ];

        let command: UserCommand = ControllerCommand::ConfigureReference {
            name: "heater".to_owned(),
            updates: updates.clone(),
            response_sender,
        }
        .into();

        assert_eq!(
            process_action_from_command(&command),
            Some(ProcessAction::ConfigureControllerReference {
                name: "heater".to_owned(),
                updates,
            },),
        );
    }

    #[test]
    fn records_controller_reference_replacement_action() {
        let (response_sender, _response_receiver) = crossbeam_channel::bounded(1);

        let source = ReferenceSource::ramp(175.0, 220.0, 2.0).unwrap();

        let command: UserCommand = ControllerCommand::SetReference {
            name: "heater".to_owned(),
            source,
            response_sender,
        }
        .into();

        assert_eq!(
            process_action_from_command(&command),
            Some(ProcessAction::SetControllerReference {
                name: "heater".to_owned(),
                source,
            },),
        );
    }

    #[test]
    fn records_controller_lifecycle_actions() {
        let (response_sender, _response_receiver) = crossbeam_channel::bounded(1);

        let command: UserCommand = ControllerCommand::SetInput {
            name: "heater".to_owned(),
            input_name: "temperature_filtered".to_owned(),
            response_sender,
        }
        .into();

        assert_eq!(
            process_action_from_command(&command),
            Some(ProcessAction::SetControllerInput {
                name: "heater".to_owned(),
                input_name: "temperature_filtered".to_owned(),
            },),
        );

        let (response_sender, _response_receiver) = crossbeam_channel::bounded(1);

        let command: UserCommand = ControllerCommand::Pause {
            name: "heater".to_owned(),
            response_sender,
        }
        .into();

        assert_eq!(
            process_action_from_command(&command),
            Some(ProcessAction::PauseController {
                name: "heater".to_owned(),
            },),
        );

        let (response_sender, _response_receiver) = crossbeam_channel::bounded(1);

        let command: UserCommand = ControllerCommand::Resume {
            name: "heater".to_owned(),
            response_sender,
        }
        .into();

        assert_eq!(
            process_action_from_command(&command),
            Some(ProcessAction::ResumeController {
                name: "heater".to_owned(),
            },),
        );

        let (response_sender, _response_receiver) = crossbeam_channel::bounded(1);

        let command: UserCommand = ControllerCommand::ResetIntegral {
            name: "heater".to_owned(),
            response_sender,
        }
        .into();

        assert_eq!(
            process_action_from_command(&command),
            Some(ProcessAction::ResetControllerIntegral {
                name: "heater".to_owned(),
            },),
        );

        let (response_sender, _response_receiver) = crossbeam_channel::bounded(1);

        let command: UserCommand = ControllerCommand::Reset {
            name: "heater".to_owned(),
            response_sender,
        }
        .into();

        assert_eq!(
            process_action_from_command(&command),
            Some(ProcessAction::ResetController {
                name: "heater".to_owned(),
            },),
        );
    }

    #[test]
    fn converts_added_generic_controller_to_process_action() {
        let descriptor = VirtualParameterDescriptor::new(
            VirtualParameterId::new(7),
            "heater_power",
            "Heater power",
            ParameterAccess::ReadWrite,
            ParameterValueType::Number,
        )
        .with_range(ParameterRange::Number {
            minimum: 0.0,
            maximum: 100.0,
        });

        let target = ControlOutputTarget::virtual_instrument(
            ConnectionId::new(3),
            VirtualInstrumentId::new(4),
            &descriptor,
        )
        .unwrap();

        let controller = OnOffController::new(150.0, 2.0, 0.0, 100.0).unwrap();

        let command: UserCommand = ControllerCommand::Add(
            NewController::new("thermostat", "temperature_filtered", target, controller).unwrap(),
        )
        .into();

        assert_eq!(
            process_action_from_command(&command,),
            Some(ProcessAction::AddController {
                connection_id: ConnectionId::new(3),

                name: "thermostat".to_owned(),

                input_name: "temperature_filtered".to_owned(),

                output_target: target.to_string(),

                kind: ControllerKind::OnOff,

                parameters: vec![
                    ("setpoint".to_owned(), InstrumentValue::Number(150.0,),),
                    ("hysteresis".to_owned(), InstrumentValue::Number(2.0,),),
                    ("output_off".to_owned(), InstrumentValue::Number(0.0,),),
                    ("output_on".to_owned(), InstrumentValue::Number(100.0,),),
                ],
            },),
        );
    }

    #[test]
    fn process_action_context_preserves_action_id() {
        let action_id = crate::process_recorder::ProcessActionId::new(42);

        let context = crate::process_recorder::ProcessActionContext::new(action_id);

        assert_eq!(context.action_id(), action_id);
    }

    #[test]
    fn initializes_runtime_from_lua_profile() {
        let directory = runtime_test_directory("initial_profile");

        let source = test_profile_source("temperature_raw", "temperature_filtered", "COM250");

        let profile = write_test_profile(&directory, "startup.lua", &source);

        let (mut runtime, log_model, _lua_events) = build_test_runtime(&profile);

        wait_for_series(&mut runtime, "temperature_filtered");

        let raw_id = runtime
            .series()
            .id_by_name("temperature_raw")
            .expect("raw series must exist");

        let filtered = runtime
            .series()
            .metadata()
            .into_iter()
            .find(|series| series.name == "temperature_filtered")
            .expect("filtered series must exist");

        assert_eq!(
            filtered.source,
            SeriesSource::Filtered {
                input: raw_id,
                definition: SignalFilterDefinition::moving_average(3).unwrap(),
            },
        );

        assert_eq!(runtime.paths().startup_script(), profile.as_path(),);

        drop(runtime);
        drop(log_model);

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rebuilds_runtime_from_another_profile() {
        let directory = runtime_test_directory("profile_rebuild");

        let first_source = test_profile_source("raw_first", "filtered_first", "COM251");

        let first_profile = write_test_profile(&directory, "first.lua", &first_source);

        let (mut runtime, log_model, _lua_events) = build_test_runtime(&first_profile);

        wait_for_series(&mut runtime, "filtered_first");

        let second_source = test_profile_source("raw_second", "filtered_second", "COM252");

        let second_profile = write_test_profile(&directory, "second.lua", &second_source);

        let (mut rebuilt, _lua_events) = runtime.rebuild_from_profile(&second_profile).unwrap();

        wait_for_series(&mut rebuilt, "filtered_second");

        assert!(rebuilt.series().id_by_name("raw_second").is_some(),);

        assert!(rebuilt.series().id_by_name("filtered_second").is_some(),);

        assert!(rebuilt.series().id_by_name("raw_first").is_none(),);

        assert!(rebuilt.series().id_by_name("filtered_first").is_none(),);

        assert_eq!(rebuilt.paths().startup_script(), second_profile.as_path(),);

        drop(rebuilt);
        drop(runtime);
        drop(log_model);

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_profile_rebuild_keeps_current_runtime() {
        let directory = runtime_test_directory("failed_profile_rebuild");

        let valid_source = test_profile_source("raw_original", "filtered_original", "COM253");

        let valid_profile = write_test_profile(&directory, "valid.lua", &valid_source);

        let (mut runtime, log_model, _lua_events) = build_test_runtime(&valid_profile);

        wait_for_series(&mut runtime, "filtered_original");

        let invalid_source = r#"
    local definition = {}

    function definition.setup()
        error(
            "Intentional E2E setup failure"
        )
    end

    return definition
    "#;

        let invalid_profile = write_test_profile(&directory, "invalid.lua", invalid_source);

        let result = runtime.rebuild_from_profile(&invalid_profile);

        let Err(error) = result else {
            panic!(
                "invalid profile rebuild \
                 unexpectedly succeeded"
            );
        };

        assert!(error.contains("Intentional E2E setup failure",), "{error}",);

        assert_eq!(runtime.paths().startup_script(), valid_profile.as_path(),);

        assert!(runtime.series().id_by_name("raw_original").is_some(),);

        assert!(runtime.series().id_by_name("filtered_original",).is_some(),);

        drop(runtime);
        drop(log_model);

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn runs_controller_configuration_lifecycle_end_to_end() {
        let directory = runtime_test_directory("controller_lifecycle");

        let source = test_profile_source("temperature_raw", "temperature_filtered", "COM254");

        let profile = write_test_profile(&directory, "startup.lua", &source);

        let (mut runtime, log_model, _lua_events) = build_test_runtime(&profile);

        wait_for_series(&mut runtime, "temperature_filtered");

        let descriptor = VirtualParameterDescriptor::new(
            VirtualParameterId::new(1),
            "heater_power",
            "Heater power",
            ParameterAccess::ReadWrite,
            ParameterValueType::Number,
        )
        .with_range(ParameterRange::Number {
            minimum: 0.0,
            maximum: 100.0,
        });

        let target = ControlOutputTarget::virtual_instrument(
            ConnectionId::PRIMARY,
            VirtualInstrumentId::new(1),
            &descriptor,
        )
        .unwrap();

        let controller = PidController::with_output_limits(
            100.0,
            PidGains::new(2.0, 0.1, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .unwrap();

        let new_controller =
            NewController::new("heater", "temperature_filtered", target, controller).unwrap();

        runtime.execute(ControllerCommand::Add(new_controller).into());

        /*
         * Controller exists and starts Running.
         */
        let (response_sender, response_receiver) = crossbeam_channel::bounded(1);

        runtime.execute(
            ControllerCommand::State {
                name: "heater".to_owned(),
                response_sender,
            }
            .into(),
        );

        assert_eq!(
            response_receiver
                .recv_timeout(Duration::from_secs(1),)
                .unwrap()
                .unwrap(),
            ControlLoopState::Running,
        );

        /*
         * Change one PID parameter.
         */
        let (response_sender, response_receiver) = crossbeam_channel::bounded(1);

        runtime.execute(
            ControllerCommand::WriteParameter {
                name: "heater".to_owned(),
                key: "setpoint".to_owned(),
                value: InstrumentValue::Number(175.0),
                response_sender,
            }
            .into(),
        );

        assert_eq!(
            response_receiver
                .recv_timeout(Duration::from_secs(1),)
                .unwrap()
                .unwrap(),
            InstrumentValue::Number(175.0),
        );

        /*
         * Change several PID parameters atomically.
         */
        let (response_sender, response_receiver) = crossbeam_channel::bounded(1);

        runtime.execute(
            ControllerCommand::Configure {
                name: "heater".to_owned(),
                updates: vec![
                    ("kp".to_owned(), InstrumentValue::Number(1.5)),
                    ("ki".to_owned(), InstrumentValue::Number(0.25)),
                    ("kd".to_owned(), InstrumentValue::Number(0.5)),
                ],
                response_sender,
            }
            .into(),
        );

        response_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap();

        /*
         * Read a configured value back through the
         * application command path.
         */
        let (response_sender, response_receiver) = crossbeam_channel::bounded(1);

        runtime.execute(
            ControllerCommand::ReadParameter {
                name: "heater".to_owned(),
                key: "kp".to_owned(),
                response_sender,
            }
            .into(),
        );

        assert_eq!(
            response_receiver
                .recv_timeout(Duration::from_secs(1),)
                .unwrap()
                .unwrap(),
            InstrumentValue::Number(1.5),
        );

        /*
         * Install a dynamic reference.
         */
        let reference = ReferenceSource::ramp(175.0, 220.0, 2.0).unwrap();

        let (response_sender, response_receiver) = crossbeam_channel::bounded(1);

        runtime.execute(
            ControllerCommand::SetReference {
                name: "heater".to_owned(),
                source: reference,
                response_sender,
            }
            .into(),
        );

        response_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap();

        /*
         * Verify reference type.
         */
        let (response_sender, response_receiver) = crossbeam_channel::bounded(1);

        runtime.execute(
            ControllerCommand::ReferenceKind {
                name: "heater".to_owned(),
                response_sender,
            }
            .into(),
        );

        assert_eq!(
            response_receiver
                .recv_timeout(Duration::from_secs(1),)
                .unwrap()
                .unwrap(),
            Some(ReferenceKind::Ramp),
        );

        /*
         * Change one reference parameter.
         */
        let (response_sender, response_receiver) = crossbeam_channel::bounded(1);

        runtime.execute(
            ControllerCommand::WriteReferenceParameter {
                name: "heater".to_owned(),
                key: "target".to_owned(),
                value: InstrumentValue::Number(230.0),
                response_sender,
            }
            .into(),
        );

        assert_eq!(
            response_receiver
                .recv_timeout(Duration::from_secs(1),)
                .unwrap()
                .unwrap(),
            InstrumentValue::Number(230.0),
        );

        /*
         * Read the reference value back.
         */
        let (response_sender, response_receiver) = crossbeam_channel::bounded(1);

        runtime.execute(
            ControllerCommand::ReadReferenceParameter {
                name: "heater".to_owned(),
                key: "target".to_owned(),
                response_sender,
            }
            .into(),
        );

        assert_eq!(
            response_receiver
                .recv_timeout(Duration::from_secs(1),)
                .unwrap()
                .unwrap(),
            InstrumentValue::Number(230.0),
        );

        /*
         * Change controller input from filtered to raw.
         */
        let (response_sender, response_receiver) = crossbeam_channel::bounded(1);

        runtime.execute(
            ControllerCommand::SetInput {
                name: "heater".to_owned(),
                input_name: "temperature_raw".to_owned(),
                response_sender,
            }
            .into(),
        );

        response_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap();

        /*
         * Controller maintenance operations.
         */
        let (response_sender, response_receiver) = crossbeam_channel::bounded(1);

        runtime.execute(
            ControllerCommand::ResetIntegral {
                name: "heater".to_owned(),
                response_sender,
            }
            .into(),
        );

        response_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap();

        let (response_sender, response_receiver) = crossbeam_channel::bounded(1);

        runtime.execute(
            ControllerCommand::Reset {
                name: "heater".to_owned(),
                response_sender,
            }
            .into(),
        );

        response_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap();

        /*
         * Reset must not change lifecycle state.
         */
        let (response_sender, response_receiver) = crossbeam_channel::bounded(1);

        runtime.execute(
            ControllerCommand::State {
                name: "heater".to_owned(),
                response_sender,
            }
            .into(),
        );

        assert_eq!(
            response_receiver
                .recv_timeout(Duration::from_secs(1),)
                .unwrap()
                .unwrap(),
            ControlLoopState::Running,
        );

        drop(runtime);
        drop(log_model);

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn lua_execution_failure_does_not_kill_runtime() {
        let directory = runtime_test_directory("lua_execution_failure");

        let source = test_profile_source("temperature_raw", "temperature_filtered", "COM255");

        let profile = write_test_profile(&directory, "startup.lua", &source);

        let (runtime, log_model, lua_events) = build_test_runtime(&profile);

        runtime
            .lua_handle()
            .execute("error('Intentional E2E runtime failure')")
            .unwrap();

        let event = lua_events.recv_timeout(Duration::from_secs(1)).unwrap();

        let LuaEvent::ExecutionFailed(error) = event else {
            panic!("expected Lua execution failure");
        };

        assert!(
            error.contains("Intentional E2E runtime failure",),
            "{error}",
        );

        /*
         * A normal Lua error must not terminate
         * the persistent Lua worker.
         */
        runtime.lua_handle().execute("return 42").unwrap();

        assert_eq!(
            lua_events.recv_timeout(Duration::from_secs(1),).unwrap(),
            LuaEvent::ExecutionSucceeded(vec!["42".to_owned()],),
        );

        drop(runtime);
        drop(log_model);

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn suspends_failed_polling_retries_and_shuts_down_cleanly() {
        let directory = runtime_test_directory("polling_failure");

        /*
         * This is intentionally not a valid system
         * serial-port name.
         */
        let missing_port = "__com_port_reader_missing_port__";

        let source = format!(
            r#"
    local definition = {{
        application = {{
            poll_interval = 0.05,
        }},

        connections = {{
            primary = {{
                port = "{missing_port}",
                timeout = 0.05,
            }},
        }},
    }}

    function definition.setup()
        app.add_serial(
            "get",
            {{
                name = "broken_series",
                interval = 0.05,
            }}
        )
    end

    return definition
    "#
        );

        let profile = write_test_profile(&directory, "startup.lua", &source);

        let (mut runtime, mut log_model, _lua_events) = build_test_runtime(&profile);

        wait_for_series(&mut runtime, "broken_series");

        /*
         * Starting acquisition itself succeeds.
         * The COM port is opened lazily when the
         * first sample is requested.
         */
        runtime.execute(UserCommand::Acquisition(AcquisitionCommand::Start));

        let running_deadline = Instant::now() + Duration::from_secs(1);

        while !runtime.is_running() {
            runtime.poll();

            assert!(
                Instant::now() < running_deadline,
                "acquisition did not start",
            );

            thread::sleep(Duration::from_millis(5));
        }

        /*
         * Three consecutive reads from the missing
         * port must suspend only this series.
         */
        wait_for_polling_state(
            &mut runtime,
            &mut log_model,
            "broken_series",
            SeriesPollingState::Suspended,
        );

        /*
         * A failed series must not stop the whole
         * acquisition worker.
         */
        assert!(runtime.is_running());

        let first_suspension_count = log_model
            .entries()
            .iter()
            .filter(|entry| {
                let text = entry.text();

                text.contains("broken_series")
                    && text.contains(
                        "polling was suspended \
                             after three consecutive \
                             polling failures",
                    )
            })
            .count();

        assert!(
            first_suspension_count >= 1,
            "polling suspension was not logged",
        );

        /*
         * Retry must re-enable the series and refresh
         * the worker schedule.
         */
        runtime.execute(
            SeriesCommand::Retry {
                name: "broken_series".to_owned(),
            }
            .into(),
        );

        /*
         * Because the port is still missing, the
         * retried series must eventually suspend
         * for a second time.
         */
        let retry_deadline = Instant::now() + Duration::from_secs(3);

        loop {
            runtime.poll();
            log_model.poll();

            let suspension_count = log_model
                .entries()
                .iter()
                .filter(|entry| {
                    let text = entry.text();

                    text.contains("broken_series")
                        && text.contains(
                            "polling was suspended \
                             after three consecutive \
                             polling failures",
                        )
                })
                .count();

            if suspension_count > first_suspension_count {
                break;
            }

            assert!(
                Instant::now() < retry_deadline,
                "retried polling did not fail \
                 and suspend again",
            );

            thread::sleep(Duration::from_millis(5));
        }

        assert!(runtime.is_running());

        /*
         * Deliberately do NOT stop acquisition here.
         * Dropping ApplicationRuntime must shut down
         * and join all its worker threads itself.
         */
        drop(runtime);
        drop(log_model);

        fs::remove_dir_all(directory).unwrap();
    }
}
