use eframe::egui;
use egui_extras::{Size, StripBuilder};

use crate::{
    app_log::LogModel,
    application_definition::ApplicationDefinition,
    application_paths::ApplicationPaths,
    application_runtime::ApplicationRuntime,
    components::{
        control_panel_model::ControlPanelModel, control_panel_view, controls_view,
        help_model::HelpModel, help_view, log_view, lua_console_model::LuaConsoleModel,
        lua_console_view, plot_model::PlotModel, plot_view, series_view,
        settings_model::SettingsModel, settings_view,
    },
    lua_application_definition::load_lua_definition_or_base,
    process_recorder::{
        NullProcessRecordWriter, ProcessRecorder, SqliteProcessRecordWriter,
        new_process_database_path,
    },
};

const SERIES_PANEL_WIDTH: f32 = 150.0;
const TOGGLE_WIDTH: f32 = 22.0;

pub struct MyApp {
    runtime: ApplicationRuntime,
    plot: PlotModel,
    series_panel_open: bool,
    log: LogModel,
    help: HelpModel,
    lua_console: LuaConsoleModel,
    log_panel_open: bool,
    control_panels: ControlPanelModel,
    control_panel_open: bool,
    settings: SettingsModel,
}

impl MyApp {
    pub fn new(application_paths: ApplicationPaths) -> Self {
        let base_definition = ApplicationDefinition::default();

        let loaded_definition =
            load_lua_definition_or_base(application_paths.startup_script(), &base_definition);

        let (definition, startup_source, lua_definition_warning) = loaded_definition.into_parts();

        let startup_script_missing = startup_source.is_none() && lua_definition_warning.is_none();

        let requested_database_path =
            new_process_database_path(application_paths.resolve("processes"));

        let (process_recorder, active_database_path, process_recorder_warning) =
            match SqliteProcessRecordWriter::create(&requested_database_path) {
                Ok(writer) => (
                    ProcessRecorder::spawn(writer)
                        .expect("failed to spawn process recorder thread"),
                    Some(requested_database_path),
                    None,
                ),

                Err(error) => (
                    ProcessRecorder::spawn(NullProcessRecordWriter)
                        .expect("failed to spawn fallback process recorder thread"),
                    None,
                    Some(format!("SQLite process recording is disabled: {error}",)),
                ),
            };

        let (log, log_handle) =
            LogModel::new(application_paths.resolve("logs"), process_recorder.clone());

        if let Some(path) = active_database_path {
            log_handle.info(format!("Process database: {}", path.display(),));
        }

        if let Some(warning) = process_recorder_warning {
            log_handle.error(warning);
        }

        if let Some(warning) = lua_definition_warning {
            log_handle.error(warning);
        }

        if startup_script_missing {
            log_handle.error(format!(
                "Lua startup file '{}' was not found. \
                 Internal defaults are active; serial \
                 connections and emulator are not configured.",
                application_paths.startup_script().display(),
            ));
        }

        let (runtime, lua_event_receiver) = ApplicationRuntime::build(
            definition,
            log_handle.clone(),
            process_recorder,
            application_paths,
            startup_source,
        )
        .expect("failed to build application runtime");

        let control_panels = ControlPanelModel::new(&[]);

        let lua_console = LuaConsoleModel::new(
            runtime.lua_handle(),
            lua_event_receiver,
            runtime.paths().resolve("lua_scripts"),
            log_handle,
        );

        Self {
            runtime,
            plot: PlotModel::new(),
            series_panel_open: false,
            log,
            help: HelpModel::default(),
            lua_console,
            log_panel_open: false,
            control_panels,
            control_panel_open: false,
            settings: SettingsModel::default(),
        }
    }

    fn reload_runtime(&mut self) -> Result<(), String> {
        if self.lua_console.is_pending() {
            return Err("Wait for the current Lua command to finish \
                 before reloading startup.lua."
                .to_owned());
        }

        let (runtime, lua_event_receiver) = self.runtime.rebuild_from_startup()?;

        self.lua_console
            .replace_worker(runtime.lua_handle(), lua_event_receiver);

        self.runtime = runtime;

        self.control_panels.replace_definitions(&[]);
        self.control_panel_open = false;

        self.plot = PlotModel::new();

        Ok(())
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.runtime.poll();
        self.lua_console.poll_events();
        self.log.poll();

        egui::Panel::top("application_menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                lua_console_view::show_menu_button(ui, &mut self.lua_console);
                control_panel_view::show_menu_button(
                    ui,
                    &self.control_panels,
                    &mut self.control_panel_open,
                );
                settings_view::show_menu_button(ui, &mut self.settings);
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

            controls_view::show(ui, &mut self.runtime);

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
                                self.runtime.series(),
                                self.runtime
                                    .definition()
                                    .runtime()
                                    .max_plot_points_per_series(),
                                self.runtime
                                    .definition()
                                    .runtime()
                                    .plot_window()
                                    .as_secs_f64(),
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
                                self.runtime.series(),
                                &self.runtime,
                                &mut self.plot,
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
                                self.runtime.series(),
                                self.runtime
                                    .definition()
                                    .runtime()
                                    .max_plot_points_per_series(),
                                self.runtime
                                    .definition()
                                    .runtime()
                                    .plot_window()
                                    .as_secs_f64(),
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

        control_panel_view::show_viewport(ui, &self.control_panels, &mut self.control_panel_open);

        settings_view::show_window(ui.ctx(), &mut self.settings, &self.runtime);

        if self.settings.take_reload_request() {
            let result = self.reload_runtime();

            self.settings.set_reload_result(result);
        }

        help_view::show_window(ui.ctx(), &mut self.help);

        ui.ctx()
            .request_repaint_after(self.runtime.definition().runtime().repaint_interval());
    }
}
