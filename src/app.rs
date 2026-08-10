use eframe::egui;
use egui_extras::{Size, StripBuilder};

use crate::{
    app_config::{AppConfig, CONFIG_PATH},
    app_log::{LogHandle, LogModel},
    application_definition::ApplicationDefinition,
    components::{
        command_model::CommandModel, controls_model::ControlsModel, controls_view,
        device_emulator_model::DeviceEmulatorModel, help_model::HelpModel, help_view, log_view,
        lua_console_model::LuaConsoleModel, lua_console_view, plot_model::PlotModel, plot_view,
        serial_settings_model::SerialSettingsModel, serial_settings_view, series_view,
    },
    connection::ConnectionId,
    data::SeriesStore,
    lua_worker::LuaWorker,
    sample_sink::NullSampleSink,
    serial_connection::SerialConnectionRegistry,
    user_command::UserCommand,
    worker::{ConnectionWorkers, WorkerConfig, WorkerHandle, spawn_serial_connection_worker},
};

const SERIES_PANEL_WIDTH: f32 = 150.0;
const TOGGLE_WIDTH: f32 = 22.0;

pub struct MyApp {
    controls: ControlsModel,
    plot: PlotModel,
    command: CommandModel,
    series: SeriesStore,
    worker_handle: WorkerHandle,
    series_panel_open: bool,
    serial_settings: SerialSettingsModel,
    log: LogModel,
    log_handle: LogHandle,
    device_emulator: DeviceEmulatorModel,
    help: HelpModel,
    config: AppConfig,
    definition: ApplicationDefinition,
    lua_console: LuaConsoleModel,
    lua_command_receiver: crossbeam_channel::Receiver<UserCommand>,
    log_panel_open: bool,

    // Must be dropped after the application-command
    // receiver so pending synchronous Lua operations
    // are disconnected before the Lua thread is joined.
    _lua_worker: LuaWorker,
}

impl MyApp {
    pub fn new() -> Self {
        let (config, config_warning) = AppConfig::load_or_default(CONFIG_PATH);

        let definition = ApplicationDefinition::try_from(&config).expect(
            "loaded application configuration must \
                 produce a valid application definition",
        );

        let (log, log_handle) = LogModel::new();

        let (lua_event_sender, lua_event_receiver) = crossbeam_channel::unbounded();

        let (lua_command_sender, lua_command_receiver) = crossbeam_channel::unbounded();

        let lua_worker = LuaWorker::spawn(lua_event_sender, lua_command_sender)
            .expect("failed to spawn Lua worker thread");

        let lua_console =
            LuaConsoleModel::new(lua_worker.handle(), lua_event_receiver, log_handle.clone());

        if let Some(warning) = config_warning {
            log_handle.error(warning);
        }

        let device_emulator = DeviceEmulatorModel::new(
            &config.serial.emulator_port,
            &config.emulator.script_path,
            log_handle.clone(),
        );

        let series = SeriesStore::new();

        let (event_sender, event_receiver) = crossbeam_channel::unbounded();

        let serial_connections = SerialConnectionRegistry::new();

        let serial_config_store = serial_connections.primary();

        let serial_settings =
            SerialSettingsModel::new(ConnectionId::PRIMARY, serial_connections, &config.serial);

        let worker_config = WorkerConfig::new(definition.runtime().default_poll_interval());

        let worker = spawn_serial_connection_worker(
            serial_config_store,
            event_sender,
            series.clone(),
            Box::new(NullSampleSink::new()),
            worker_config,
        );

        let workers = ConnectionWorkers::new(worker);

        let connection_router = workers.router();

        let worker_handle = connection_router.handle(ConnectionId::PRIMARY).expect(
            "primary worker was registered \
                 during construction",
        );

        let controls = ControlsModel::new(workers, log_handle.clone());

        let command = CommandModel::new(connection_router, event_receiver, log_handle.clone());

        Self {
            config,
            definition,
            controls,
            plot: PlotModel::new(),
            command,
            series,
            worker_handle,
            series_panel_open: false,
            serial_settings,
            log,
            log_handle,
            device_emulator,
            help: HelpModel::default(),
            lua_console,
            lua_command_receiver,
            log_panel_open: false,
            _lua_worker: lua_worker,
        }
    }

    fn poll_lua_commands(&mut self) {
        let commands = self.lua_command_receiver.try_iter().collect::<Vec<_>>();

        for command in commands {
            self.command.execute(
                command,
                &mut self.controls,
                &self.serial_settings,
                &mut self.device_emulator,
            );
        }
    }

    fn refresh_application_definition(&mut self) {
        self.definition = ApplicationDefinition::try_from(&self.config).expect(
            "settings UI must preserve a valid \
                     application configuration",
        );
    }
}

impl Default for MyApp {
    fn default() -> Self {
        Self::new()
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.device_emulator.poll();
        self.command.poll_events(&mut self.controls);
        self.poll_lua_commands();
        self.lua_console.poll_events();
        self.log.poll();

        egui::Panel::top("application_menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                lua_console_view::show_menu_button(ui, &mut self.lua_console);

                serial_settings_view::show_menu_button(ui, &mut self.serial_settings);

                help_view::show_menu_button(ui, &mut self.help);
            });
        });

        if self.log_panel_open {
            egui::Panel::bottom("application_log_content_v3")
                .resizable(false)
                .size_range(150.0..=150.0)
                .show(ui, |ui| {
                    log_view::show_entries(ui, &mut self.log);
                });
        }

        egui::Panel::bottom("application_log_header")
            .resizable(false)
            .show(ui, |ui| {
                log_view::show_header(ui, &mut self.log_panel_open);
            });

        egui::CentralPanel::default().show(ui, |ui| {
            lua_console_view::show_panel(ui, &mut self.lua_console);

            controls_view::show(
                ui,
                &mut self.controls,
                &self.command,
                &self.serial_settings,
                &mut self.device_emulator,
            );

            ui.separator();

            if self.series_panel_open {
                StripBuilder::new(ui)
                    .size(Size::remainder())
                    .size(Size::exact(TOGGLE_WIDTH))
                    .size(Size::exact(SERIES_PANEL_WIDTH))
                    .horizontal(|mut strip| {
                        strip.cell(|ui| {
                            plot_view::show(
                                ui,
                                &mut self.plot,
                                &self.series,
                                self.definition.runtime().max_plot_points_per_series(),
                                self.definition.runtime().plot_window().as_secs_f64(),
                            );
                        });

                        strip.cell(|ui| {
                            if ui.button("◀").clicked() {
                                self.series_panel_open = false;
                            }
                        });

                        strip.cell(|ui| {
                            series_view::show(ui, &self.series, &self.command, &mut self.plot);
                        });
                    });
            } else {
                StripBuilder::new(ui)
                    .size(Size::remainder())
                    .size(Size::exact(TOGGLE_WIDTH))
                    .horizontal(|mut strip| {
                        strip.cell(|ui| {
                            plot_view::show(
                                ui,
                                &mut self.plot,
                                &self.series,
                                self.definition.runtime().max_plot_points_per_series(),
                                self.definition.runtime().plot_window().as_secs_f64(),
                            );
                        });

                        strip.cell(|ui| {
                            if ui.button("▶").clicked() {
                                self.series_panel_open = true;
                            }
                        });
                    });
            }
        });

        let acquisition_running = self.controls.is_running();

        serial_settings_view::show_window(
            ui.ctx(),
            &mut self.serial_settings,
            &mut self.device_emulator,
            &self.worker_handle,
            acquisition_running,
            &mut self.config,
            &self.log_handle,
        );

        self.refresh_application_definition();

        help_view::show_window(ui.ctx(), &mut self.help);

        ui.ctx()
            .request_repaint_after(self.definition.runtime().repaint_interval());
    }
}
