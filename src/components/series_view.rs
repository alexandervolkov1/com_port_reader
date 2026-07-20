use eframe::egui::{self, ScrollArea};

use crate::components::controls_model::ControlsModel;

pub fn show(ui: &mut egui::Ui, controls: &mut ControlsModel) {
    ScrollArea::vertical().show(ui, |ui| {
        let Ok(mut series) = controls.series().lock() else {
            return;
        };

        let mut remove_idx = None;

        for (idx, signal) in series.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.checkbox(&mut signal.visible, "");
                ui.label(signal.signal.to_string());

                if ui.button("Delete").clicked() {
                    remove_idx = Some(idx);
                }
            });
        }
        if let Some(idx) = remove_idx {
            series.remove(idx);
        }
    });
}
