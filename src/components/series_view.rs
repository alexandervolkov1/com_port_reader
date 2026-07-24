use eframe::egui::{self, ScrollArea};

use crate::{
    components::{
        command_model::CommandModel, plot_model::PlotModel, series_editor_model::SeriesEditorModel,
    },
    data::{SeriesId, SeriesStore},
};

pub fn show(
    ui: &mut egui::Ui,
    series_store: &SeriesStore,
    commands: &CommandModel,
    plot: &mut PlotModel,
    editor: &mut SeriesEditorModel,
) {
    if ui.button("Add new").clicked() {
        editor.open();
    }

    ui.separator();

    ScrollArea::vertical().show(ui, |ui| {
        let series = series_store.metadata();

        let pane_ids = plot.panes.iter().map(|pane| pane.id).collect::<Vec<_>>();

        let mut remove_id: Option<SeriesId> = None;

        for series in series {
            let mut visible = series.visible;

            let current_pane = plot.pane_for_series(series.id);

            let mut selected_pane = current_pane;

            let selected_number = pane_ids
                .iter()
                .position(|pane_id| *pane_id == selected_pane)
                .map_or(1, |index| index + 1);

            ui.group(|ui| {
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut visible, "").changed() {
                        commands.set_visibility(series.id, visible);
                    }

                    ui.label(&series.name)
                        .on_hover_text(series.source.to_string());

                    if ui.button("Delete").clicked() {
                        remove_id = Some(series.id);
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Plot:");

                    egui::ComboBox::from_id_salt(("series_plot_pane", series.id))
                        .selected_text(format!("Plot {selected_number}",))
                        .show_ui(ui, |ui| {
                            for (index, pane_id) in pane_ids.iter().copied().enumerate() {
                                ui.selectable_value(
                                    &mut selected_pane,
                                    pane_id,
                                    format!("Plot {}", index + 1,),
                                );
                            }
                        });
                });
            });

            if selected_pane != current_pane {
                plot.assign_series(series.id, selected_pane);
            }
        }

        if let Some(id) = remove_id {
            commands.remove_series(id);
        }
    });
}
