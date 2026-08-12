use std::{fs, path::Path};

use crossbeam_channel::{unbounded, Receiver};

use crate::{
    app_log::LogHandle,
    application_definition::ApplicationDefinition,
    application_paths::ApplicationPaths,
    connection::ConnectionId,
    data::{SeriesId, SeriesStore},
    lua_application_definition::apply_lua_definition,
    lua_worker::{LuaEvent, LuaWorker, LuaWorkerHandle},
    sample_sink::NullSampleSink,
    serial_connection::SerialConnectionRegistry,
    user_command::UserCommand,
    worker::{spawn_serial_connection_worker, ConnectionWorkers, WorkerConfig},
};

mod acquisition_controller;
mod command_dispatcher;
mod device_emulator_service;

pub(crate) use acquisition_controller::{AcquisitionController, RecordingTransition};
pub(crate) use command_dispatcher::CommandDispatcher;
pub(crate) use device_emulator_service::DeviceEmulatorService;

pub struct ApplicationRuntime {
    lua_worker: LuaWorker,
    definition: ApplicationDefinition,
    series: SeriesStore,
    acquisition: AcquisitionController,
    dispatcher: CommandDispatcher,
    device_emulator: DeviceEmulatorService,
    lua_command_receiver: Receiver<UserCommand>,
    paths: ApplicationPaths,
}

impl ApplicationRuntime {
    pub(crate) fn build(
        definition: ApplicationDefinition,
        log: LogHandle,
        paths: ApplicationPaths,
        startup_source: Option<String>,
    ) -> std::io::Result<(Self, Receiver<LuaEvent>)> {
        let (lua_event_sender, lua_event_receiver) = unbounded();

        let (lua_command_sender, lua_command_receiver) = unbounded();

        let lua_worker = LuaWorker::spawn(
            lua_event_sender,
            lua_command_sender,
            definition.clone(),
            startup_source,
        )?;

        let series = SeriesStore::new();

        let emulator_port = definition
            .emulator()
            .map(|emulator| emulator.port_name().to_owned());

        let emulator_script_path = definition
            .emulator()
            .map(|emulator| paths.resolve(emulator.script_path()));

        let recording_directory = paths.resolve("protocols");

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
                    "validated application definition \
                     must contain unique connection IDs",
                )
            };

            store.set(Some(connection.serial_config().clone()));
        }

        let worker_config = WorkerConfig::new(definition.runtime().default_poll_interval());

        let primary_worker = spawn_serial_connection_worker(
            serial_connections.primary(),
            event_sender.clone(),
            series.clone(),
            Box::new(NullSampleSink::new()),
            worker_config,
        );

        let mut workers = ConnectionWorkers::new(primary_worker);

        for connection in definition.serial_connections() {
            let connection_id = connection.id();

            if connection_id == ConnectionId::PRIMARY {
                continue;
            }

            let config_store = serial_connections.store(connection_id).expect(
                "serial connection store was registered \
                 before spawning its worker",
            );

            let worker = spawn_serial_connection_worker(
                config_store,
                event_sender.clone(),
                series.clone(),
                Box::new(NullSampleSink::new()),
                worker_config,
            );

            workers.insert(worker).expect(
                "validated application definition must \
                 contain unique connection IDs",
            );
        }

        let connection_router = workers.router();

        let acquisition = AcquisitionController::new(workers, recording_directory, log.clone());

        let dispatcher = CommandDispatcher::new(
            connection_router,
            serial_connections,
            definition.clone(),
            event_receiver,
            log,
        );

        let runtime = Self::new(
            lua_worker,
            definition,
            paths,
            series,
            acquisition,
            dispatcher,
            device_emulator,
            lua_command_receiver,
        );

        Ok((runtime, lua_event_receiver))
    }

    fn new(
        lua_worker: LuaWorker,
        definition: ApplicationDefinition,
        paths: ApplicationPaths,
        series: SeriesStore,
        acquisition: AcquisitionController,
        dispatcher: CommandDispatcher,
        device_emulator: DeviceEmulatorService,
        lua_command_receiver: Receiver<UserCommand>,
    ) -> Self {
        Self {
            lua_worker,
            definition,
            paths,
            series,
            acquisition,
            dispatcher,
            device_emulator,
            lua_command_receiver,
        }
    }

    pub fn poll(&mut self) {
        self.device_emulator.poll();

        self.dispatcher.poll_events(&mut self.acquisition);

        let commands = self.lua_command_receiver.try_iter().collect::<Vec<_>>();

        for command in commands {
            self.execute(command);
        }
    }

    pub fn execute(&mut self, command: UserCommand) {
        self.dispatcher
            .execute(command, &mut self.acquisition, &mut self.device_emulator);
    }

    pub fn is_running(&self) -> bool {
        self.acquisition.is_running()
    }

    pub fn is_recording(&self) -> bool {
        self.acquisition.is_recording()
    }

    pub fn recording_transition(&self) -> Option<RecordingTransition> {
        self.acquisition.recording_transition()
    }

    pub fn recording_file(&self) -> Option<&Path> {
        self.acquisition.recording_file()
    }

    pub fn recording_error(&self) -> Option<&str> {
        self.acquisition.recording_error()
    }

    pub fn set_series_visibility(&self, id: SeriesId, visible: bool) {
        self.dispatcher.set_visibility(id, visible);
    }

    pub fn remove_series(&self, id: SeriesId) {
        self.dispatcher.remove_series(id);
    }

    pub(crate) fn lua_handle(&self) -> LuaWorkerHandle {
        self.lua_worker.handle()
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
        let path = self.paths.startup_script();

        let source = fs::read_to_string(path).map_err(|error| {
            format!("Failed to read startup file '{}': {error}", path.display(),)
        })?;

        apply_lua_definition(&source, &ApplicationDefinition::default()).map_err(|error| {
            format!(
                "Failed to validate startup file '{}': \
                 {error}",
                path.display(),
            )
        })?;

        Ok(())
    }

    pub(crate) fn open_startup_configuration(
        &self,
    ) -> Result<(), String> {
        let path = self.paths.startup_script();

        if !path.is_file() {
            return Err(format!(
                "Startup file '{}' does not exist",
                path.display(),
            ));
        }

        open::that(path).map_err(|error| {
            format!(
                "Failed to open startup file '{}': {error}",
                path.display(),
            )
        })
    }
}
