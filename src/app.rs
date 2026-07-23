use eframe::egui;
use egui_extras::{Size, StripBuilder};
use std::time::Duration;

use crate::acquisition::{CombinedSource, SerialCommandSource, SignalGenerator};
use crate::components::{
    command_model::CommandModel, command_view, controls_model::ControlsModel, controls_view,
    plot_model::PlotModel, plot_view, script_model::ScriptModel, script_view,
    serial_settings_model::SerialSettingsModel, serial_settings_view,
    series_editor_model::SeriesEditorModel, series_editor_view, series_view,
};
use crate::data::SeriesStore;
use crate::sample_sink::NullSampleSink;
use crate::serial_connection::SerialConfigStore;
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
}

impl MyApp {
    pub fn new() -> Self {
        let (log, log_handle) = LogModel::new();
        let series = SeriesStore::new();
        let (command_sender, command_receiver) = crossbeam_channel::bounded(32);
        let (event_sender, event_receiver) = crossbeam_channel::unbounded();
        let worker_handle = WorkerHandle::new(command_sender);
        let serial_config_store = SerialConfigStore::new();
        let serial_settings = SerialSettingsModel::new(serial_config_store.clone());
        let worker_config = WorkerConfig::new(Duration::from_millis(1000));
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
        );

        let command = CommandModel::new(worker_handle.clone(), event_receiver, log_handle.clone());
        Self {
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
        }
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.command.poll_events();
        self.log.poll();

        egui::Panel::bottom("application_log")
            .resizable(true)
            .default_size(150.0)
            .min_size(80.0)
            .show(ui, |ui| {
                log_view::show(ui, &mut self.log);
            });

        egui::CentralPanel::default().show(ui, |ui| {
            controls_view::show(ui, &mut self.controls);

            ui.separator();

            command_view::show(ui, &mut self.command);

            script_view::show(
                ui,
                &mut self.script,
                &mut self.command,
                &self.controls,
                &self.log_handle,
            );

            ui.separator();

            serial_settings_view::show(ui, &mut self.serial_settings, &self.worker_handle);
            ui.separator();

            if self.series_panel_open {
                StripBuilder::new(ui)
                    .size(Size::remainder())
                    .size(Size::exact(TOGGLE_WIDTH))
                    .size(Size::exact(SERIES_PANEL_WIDTH))
                    .horizontal(|mut strip| {
                        strip.cell(|ui| {
                            plot_view::show(ui, &mut self.plot, &self.series);
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
                                &self.worker_handle,
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
                            plot_view::show(ui, &mut self.plot, &self.series);
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

        ui.ctx().request_repaint_after(Duration::from_millis(33));
    }
}
