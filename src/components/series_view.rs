use eframe::egui::{self, ScrollArea};

use crate::{
    application_runtime::ApplicationRuntime,
    components::plot_model::PlotModel,
    data::{SeriesPollingState, SeriesStore},
};

pub fn show(
    ui: &mut egui::Ui,
    series_store: &SeriesStore,
    runtime: &ApplicationRuntime,
    plot: &mut PlotModel,
) {
    ScrollArea::vertical().show(ui, |ui| {
        let series = series_store.metadata();

        let pane_ids = plot.panes.iter().map(|pane| pane.id).collect::<Vec<_>>();

        for series in series {
            let mut visible = series.visible;

            let current_pane = plot.pane_for_series(series.id);

            let mut selected_pane = current_pane;

            let selected_number = pane_ids
                .iter()
                .position(|pane_id| *pane_id == selected_pane)
                .map_or(1, |index| index + 1);

            ui.group(|ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.checkbox(&mut visible, "").changed() {
                        runtime.set_series_visibility(series.id, visible);
                    }

                    ui.label(&series.name)
                        .on_hover_text(series.source.to_string());

                    if series.polling_state == SeriesPollingState::Suspended {
                        ui.colored_label(offline_color(ui), "Offline")
                            .on_hover_text(
                                "Periodic polling is suspended. \
                             Use the instrument Refresh button \
                             or restart acquisition to retry. \
                             Existing samples remain on the plot.",
                            );
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
    });
}

fn offline_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(255, 100, 100)
    } else {
        egui::Color32::from_rgb(180, 30, 30)
    }
}
