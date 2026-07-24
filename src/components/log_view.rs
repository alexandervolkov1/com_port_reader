use eframe::egui;

use crate::app_log::{LogLevel, LogModel};

pub fn show(ui: &mut egui::Ui, model: &mut LogModel) {
    ui.horizontal(|ui| {
        ui.strong("Application log");

        if ui.button("Clear").clicked() {
            model.clear();
        }
    });

    ui.separator();

    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for entry in model.entries() {
                let color = entry_color(ui, entry.level());

                ui.colored_label(color, entry.text());
            }
        });
}

fn entry_color(ui: &egui::Ui, level: LogLevel) -> egui::Color32 {
    match level {
        LogLevel::Info => ui.visuals().text_color(),

        LogLevel::Error if ui.visuals().dark_mode => egui::Color32::from_rgb(255, 100, 100),

        LogLevel::Error => egui::Color32::from_rgb(170, 20, 20),
    }
}
