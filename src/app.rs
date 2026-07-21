use eframe::egui;
use egui_extras::{Size, StripBuilder};
use std::time::Duration;

use crate::components::{
    command_model::CommandModel, command_view, controls_model::ControlsModel, controls_view,
    plot_model::PlotModel, plot_view, series_view,
};
use crate::data::SeriesStore;
use crate::worker::WorkerHandle;

pub struct MyApp {
    controls: ControlsModel,
    plot: PlotModel,
    command: CommandModel,
    series: SeriesStore,
    worker_handle: WorkerHandle,

    series_panel_open: bool,
}

impl MyApp {
    pub fn new() -> Self {
        let series = SeriesStore::new();

        let (command_sender, command_receiver) = crossbeam_channel::bounded(32);

        let (response_sender, response_receiver) = crossbeam_channel::bounded(32);

        let worker_handle = WorkerHandle::new(command_sender);

        let controls = ControlsModel::new(
            series.clone(),
            worker_handle.clone(),
            command_receiver,
            response_sender,
        );

        let command = CommandModel::new(worker_handle.clone(), response_receiver);
        Self {
            controls,
            plot: PlotModel::new(),
            command,
            series,
            worker_handle,
            series_panel_open: false,
        }
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.command.poll_response();

        egui::CentralPanel::default().show(ui, |ui| {
            controls_view::show(ui, &mut self.controls);

            ui.separator();

            command_view::show(ui, &mut self.command);

            ui.separator();

            const SERIES_PANEL_WIDTH: f32 = 300.0;
            const TOGGLE_WIDTH: f32 = 22.0;

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
                            series_view::show(ui, &self.series, &self.worker_handle);
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
        });

        ui.ctx().request_repaint_after(Duration::from_millis(33));
    }
}
