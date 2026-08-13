use eframe::egui;
use egui_extras::{Size, StripBuilder};

use crate::app_log::{LogLevel, LogModel};

pub fn show_header(ui: &mut egui::Ui, panel_open: &mut bool) {
    ui.horizontal(|ui| {
        ui.strong("Application log");

        let arrow = if *panel_open { "◀" } else { "▶" };

        if ui.button(arrow).clicked() {
            *panel_open = !*panel_open;
        }
    });
}

pub fn show_entries(ui: &mut egui::Ui, model: &mut LogModel) {
    let controls_height = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;

    StripBuilder::new(ui)
        .size(Size::remainder())
        .size(Size::exact(controls_height))
        .vertical(|mut strip| {
            strip.cell(|ui| {
                egui::ScrollArea::vertical()
                    .id_salt("application_log_entries")
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());

                        ui.add_space(4.0);

                        for entry in model.entries() {
                            let color = entry_color(ui, entry.level());

                            ui.colored_label(color, entry.text());
                        }
                    });
            });

            strip.cell(|ui| {
                ui.separator();

                if ui.button("Clear").clicked() {
                    model.clear();
                }
            });
        });
}

fn entry_color(ui: &egui::Ui, level: LogLevel) -> egui::Color32 {
    match level {
        LogLevel::Info => ui.visuals().text_color(),

        LogLevel::Error if ui.visuals().dark_mode => egui::Color32::from_rgb(255, 100, 100),

        LogLevel::Error => egui::Color32::from_rgb(170, 20, 20),
    }
}
