use eframe::egui::{self, ScrollArea};

use crate::data::SeriesStore;

pub fn show(ui: &mut egui::Ui, series_store: &SeriesStore) {
    ScrollArea::vertical().show(ui, |ui| {
        series_store.with_mut(|series| {
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
    });
}
