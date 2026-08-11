use std::path::{Path, PathBuf};

use crossbeam_channel::Receiver;

use crate::{
    app_log::LogHandle,
    application_definition::ApplicationDefinition,
    connection::ConnectionId,
    data::{SeriesId, SeriesStore},
    sample_sink::NullSampleSink,
    serial_connection::SerialConnectionRegistry,
    user_command::UserCommand,
    worker::{ConnectionWorkers, WorkerConfig, spawn_serial_connection_worker},
};

mod acquisition_controller;
mod command_dispatcher;
mod device_emulator_service;

pub(crate) use acquisition_controller::{AcquisitionController, RecordingTransition};
pub(crate) use command_dispatcher::CommandDispatcher;
pub(crate) use device_emulator_service::DeviceEmulatorService;

pub struct ApplicationRuntime {
    acquisition: AcquisitionController,
    dispatcher: CommandDispatcher,
    device_emulator: DeviceEmulatorService,
    lua_command_receiver: Receiver<UserCommand>,
}

impl ApplicationRuntime {
    pub(crate) fn build(
        definition: &ApplicationDefinition,
        series: SeriesStore,
        log: LogHandle,
        emulator_script_path: Option<PathBuf>,
        lua_command_receiver: Receiver<UserCommand>,
    ) -> Self {
        let emulator_port = definition
            .emulator()
            .map(|emulator| emulator.port_name().to_owned());

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

        let acquisition = AcquisitionController::new(workers, log.clone());

        let dispatcher = CommandDispatcher::new(
            connection_router,
            serial_connections,
            definition.clone(),
            event_receiver,
            log,
        );

        Self::new(
            acquisition,
            dispatcher,
            device_emulator,
            lua_command_receiver,
        )
    }

    pub(crate) fn new(
        acquisition: AcquisitionController,
        dispatcher: CommandDispatcher,
        device_emulator: DeviceEmulatorService,
        lua_command_receiver: Receiver<UserCommand>,
    ) -> Self {
        Self {
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
}
