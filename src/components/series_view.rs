use eframe::egui::{self, ScrollArea};

use crate::data::{SeriesId, SeriesStore};

pub fn show(ui: &mut egui::Ui, series_store: &SeriesStore) {
    ScrollArea::vertical().show(ui, |ui| {
        let series = series_store.metadata();
        let mut remove_id: Option<SeriesId> = None;

        for series in series {
            let mut visible = series.visible;

            ui.horizontal(|ui| {
                if ui.checkbox(&mut visible, "").changed() {
                    series_store.set_visibility(series.id, visible);
                }

                ui.label(series.signal.to_string());

                if ui.button("Delete").clicked() {
                    remove_id = Some(series.id);
                }
            });
        }

        if let Some(id) = remove_id {
            series_store.remove_series(id);
        }
    });
}
