use eframe::egui::{self, ScrollArea};

use crate::{
    components::series_editor_model::SeriesEditorModel,
    data::{SeriesId, SeriesStore},
    worker::WorkerHandle,
};

pub fn show(
    ui: &mut egui::Ui,
    series_store: &SeriesStore,
    worker_handle: &WorkerHandle,
    editor: &mut SeriesEditorModel,
) {
    if ui.button("Add new").clicked() {
        editor.open();
    }

    ui.separator();

    ScrollArea::vertical().show(ui, |ui| {
        let series = series_store.metadata();
        let mut remove_id: Option<SeriesId> = None;

        for series in series {
            let mut visible = series.visible;

            ui.horizontal(|ui| {
                if ui.checkbox(&mut visible, "").changed() {
                    let _ = worker_handle.set_visibility(series.id, visible);
                }

                ui.label(&series.name)
                    .on_hover_text(series.source.to_string());

                if ui.button("Delete").clicked() {
                    remove_id = Some(series.id);
                }
            });
        }

        if let Some(id) = remove_id {
            let _ = worker_handle.remove_series(id);
        }
    });
}
