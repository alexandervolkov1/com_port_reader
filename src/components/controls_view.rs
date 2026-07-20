use eframe::egui;

use crate::components::controls_model::ControlsModel;

pub fn show(ui: &mut egui::Ui, controls: &mut ControlsModel) {
    ui.horizontal(|ui| {
        if ui.button("Start").clicked() {
            controls.start();
        }

        if ui.button("Stop").clicked() {
            controls.stop();
        }

        if ui.button("Clear").clicked() {
            controls.clear();
        }

        if controls.is_running() {
            ui.colored_label(egui::Color32::from_rgb(0, 130, 0), "▶ Running");
        } else {
            ui.colored_label(egui::Color32::from_rgb(130, 0, 0), "■ Stopped");
        }
    });
}
