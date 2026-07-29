use eframe::egui;
use egui_extras::{Size, StripBuilder};

use crate::acquisition::{CombinedSource, SerialCommandSource, SignalGenerator};
use crate::app_config::{AppConfig, CONFIG_PATH};
use crate::components::{
    command_model::CommandModel, command_view, controls_model::ControlsModel, controls_view,
    device_emulator_model::DeviceEmulatorModel, help_model::HelpModel, help_view,
    lua_console_model::LuaConsoleModel, lua_console_view, plot_model::PlotModel, plot_view,
    script_model::ScriptModel, script_view, serial_settings_model::SerialSettingsModel,
    serial_settings_view, series_editor_model::SeriesEditorModel, series_editor_view, series_view,
};
use crate::data::SeriesStore;
use crate::lua_worker::LuaWorker;
use crate::sample_sink::NullSampleSink;
use crate::serial_connection::SerialConfigStore;
use crate::user_command::UserCommand;
use crate::worker::{WorkerConfig, WorkerHandle};
use crate::{
    app_log::{LogHandle, LogModel},
    components::log_view,
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
    series_editor: SeriesEditorModel,
    serial_settings: SerialSettingsModel,
    script: ScriptModel,
    log: LogModel,
    log_handle: LogHandle,
    device_emulator: DeviceEmulatorModel,
    help: HelpModel,
    config: AppConfig,
    _lua_worker: LuaWorker,
    lua_console: LuaConsoleModel,
    lua_command_receiver: crossbeam_channel::Receiver<UserCommand>,
}

impl MyApp {
    pub fn new() -> Self {
        let (config, config_warning) = AppConfig::load_or_default(CONFIG_PATH);

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

        let (command_sender, command_receiver) = crossbeam_channel::bounded(32);

        let (event_sender, event_receiver) = crossbeam_channel::unbounded();

        let worker_handle = WorkerHandle::new(command_sender);

        let serial_config_store = SerialConfigStore::new();

        let serial_settings = SerialSettingsModel::new(serial_config_store.clone(), &config.serial);

        let worker_config = WorkerConfig::new(config.application.poll_interval());

        let source = CombinedSource::new(vec![
            Box::new(SignalGenerator::new()),
            Box::new(SerialCommandSource::new(serial_config_store)),
        ]);

        let controls = ControlsModel::new(
            series.clone(),
            worker_handle.clone(),
            command_receiver,
            event_sender,
            Box::new(source),
            Box::new(NullSampleSink::new()),
            worker_config,
            log_handle.clone(),
        );

        let command = CommandModel::new(worker_handle.clone(), event_receiver, log_handle.clone());

        Self {
            config,
            controls,
            plot: PlotModel::new(),
            command,
            series,
            worker_handle,
            series_panel_open: false,
            series_editor: SeriesEditorModel::default(),
            serial_settings,
            script: ScriptModel::new(),
            log,
            log_handle,
            device_emulator,
            help: HelpModel::default(),
            _lua_worker: lua_worker,
            lua_console,
            lua_command_receiver,
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
                serial_settings_view::show_menu_button(ui, &mut self.serial_settings);

                help_view::show_menu_button(ui, &mut self.help);
            });
        });

        egui::Panel::bottom("application_log")
            .resizable(true)
            .default_size(150.0)
            .min_size(80.0)
            .show(ui, |ui| {
                log_view::show(ui, &mut self.log);
            });

        egui::CentralPanel::default().show(ui, |ui| {
            controls_view::show(
                ui,
                &mut self.controls,
                &self.command,
                &self.serial_settings,
                &mut self.device_emulator,
            );
            ui.separator();

            command_view::show(
                ui,
                &mut self.command,
                &mut self.controls,
                &self.serial_settings,
                &mut self.device_emulator,
            );

            lua_console_view::show(ui, &mut self.lua_console);

            script_view::show(
                ui,
                &mut self.script,
                &self.command,
                &mut self.controls,
                &self.serial_settings,
                &mut self.device_emulator,
                &self.log_handle,
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
                                self.config.application.max_plot_points_per_series,
                            );
                        });

                        strip.cell(|ui| {
                            if ui.button("◀").clicked() {
                                self.series_panel_open = false;
                            }
                        });

                        strip.cell(|ui| {
                            series_view::show(
                                ui,
                                &self.series,
                                &self.command,
                                &mut self.plot,
                                &mut self.series_editor,
                            );
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
                                self.config.application.max_plot_points_per_series,
                            );
                        });

                        strip.cell(|ui| {
                            if ui.button("▶").clicked() {
                                self.series_panel_open = true;
                            }
                        });
                    });
            }
            series_editor_view::show(ui.ctx(), &mut self.series_editor, &mut self.command);
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

        help_view::show_window(ui.ctx(), &mut self.help);

        ui.ctx()
            .request_repaint_after(self.config.application.repaint_interval());
    }
}
