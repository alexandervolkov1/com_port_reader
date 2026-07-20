use eframe::egui;
use egui_extras::{Size, StripBuilder};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::components::{
    command_model::CommandModel, command_view, controls_model::ControlsModel, controls_view,
    plot_model::PlotModel, plot_view, series_view,
};

pub struct MyApp {
    controls: ControlsModel,
    plot: PlotModel,
    command: CommandModel,

    series_panel_open: bool,
}

impl MyApp {
    pub fn new() -> Self {
        let series = Arc::new(Mutex::new(Vec::new()));
        let (command_sender, command_receiver) = crossbeam_channel::bounded(32);
        let (response_sender, response_receiver) = crossbeam_channel::bounded(32);
        Self {
            controls: ControlsModel::new(series, command_receiver, response_sender),
            command: CommandModel::new(command_sender, response_receiver),
            plot: PlotModel::new(),
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
                            plot_view::show(ui, &mut self.plot, &self.controls);
                        });

                        strip.cell(|ui| {
                            if ui.button("◀").clicked() {
                                self.series_panel_open = false;
                            }
                        });

                        strip.cell(|ui| {
                            series_view::show(ui, &mut self.controls);
                        });
                    });
            } else {
                StripBuilder::new(ui)
                    .size(Size::remainder())
                    .size(Size::exact(TOGGLE_WIDTH))
                    .horizontal(|mut strip| {
                        strip.cell(|ui| {
                            plot_view::show(ui, &mut self.plot, &self.controls);
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
