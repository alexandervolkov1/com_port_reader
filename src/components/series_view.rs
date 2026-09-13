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
    sidebar(ui).show(ui, |ui| {
        ui.small("Drag the left edge to resize.");
        ui.separator();
        show_contents(ui, series_store, runtime, plot);
    });
}

fn sidebar(ui: &egui::Ui) -> egui::Panel {
    // Leave room for the plot even when the application window is narrow.
    let maximum = (ui.available_width() * 0.75).max(1.0);
    egui::Panel::right("series_sidebar")
        .resizable(true)
        .default_size(300.0)
        .size_range(180.0_f32.min(maximum)..=maximum)
}

fn show_contents(
    ui: &mut egui::Ui,
    series_store: &SeriesStore,
    runtime: &ApplicationRuntime,
    plot: &mut PlotModel,
) {
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let series = series_store.metadata();

            let panes = plot
                .panes
                .iter()
                .map(|pane| (pane.id, pane.key.clone(), pane.title.clone()))
                .collect::<Vec<_>>();

            for series in series {
                let mut visible = series.presentation.visible;

                let current_pane = plot.pane_for_series(series.presentation.pane.as_ref());

                let mut selected_pane = current_pane;

                let selected_title = panes
                    .iter()
                    .find(|(pane_id, _, _)| *pane_id == selected_pane)
                    .map(|(_, _, title)| title.as_str())
                    .unwrap_or("Plot");

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
                                    "Periodic polling was suspended after \
                                 three consecutive failures. Use \
                                 app.retry(name), app.retry_all(), a \
                                 successful instrument Refresh, or stop \
                                 and start acquisition to retry. Existing \
                                 samples remain on the plot.",
                                );
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Plot:");

                        egui::ComboBox::from_id_salt(("series_plot_pane", series.id))
                            .width(ui.available_width())
                            .wrap()
                            .selected_text(selected_title)
                            .show_ui(ui, |ui| {
                                for (pane_id, _, title) in &panes {
                                    ui.selectable_value(&mut selected_pane, *pane_id, title);
                                }
                            });
                    });
                });

                if selected_pane != current_pane {
                    let pane = panes
                        .iter()
                        .find(|(pane_id, _, _)| *pane_id == selected_pane)
                        .map(|(_, key, _)| key.clone());
                    series_store.set_pane(series.id, pane);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        context: &egui::Context,
        width: f32,
        open: bool,
        events: Vec<egui::Event>,
    ) -> Option<egui::Rect> {
        let mut panel_rect = None;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 600.0),
            )),
            events,
            ..Default::default()
        };
        let _ = context.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.label("Acquisition controls");
                if open {
                    panel_rect = Some(
                        sidebar(ui)
                            .show(ui, |ui| {
                                ui.take_available_space();
                            })
                            .response
                            .rect,
                    );
                }
                assert!(
                    ui.available_width() > 0.0,
                    "panel must leave room for plots"
                );
                ui.take_available_space();
            });
        });
        panel_rect
    }

    #[test]
    fn sidebar_can_be_dragged_and_remembers_width_when_reopened() {
        let context = egui::Context::default();
        let initial = frame(&context, 1000.0, true, vec![]).unwrap();
        let edge = initial.left_center();
        frame(
            &context,
            1000.0,
            true,
            vec![egui::Event::PointerMoved(edge)],
        );
        frame(
            &context,
            1000.0,
            true,
            vec![egui::Event::PointerButton {
                pos: edge,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let moved = edge - egui::vec2(150.0, 0.0);
        frame(
            &context,
            1000.0,
            true,
            vec![egui::Event::PointerMoved(moved)],
        );
        frame(
            &context,
            1000.0,
            true,
            vec![egui::Event::PointerButton {
                pos: moved,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let expanded = frame(&context, 1000.0, true, vec![]).unwrap();
        assert!(
            expanded.width() > initial.width() + 100.0,
            "{initial:?} -> {expanded:?}"
        );
        frame(&context, 1000.0, false, vec![]);
        let reopened = frame(&context, 1000.0, true, vec![]).unwrap();
        assert!((reopened.width() - expanded.width()).abs() < 1.0);
        let narrow = frame(&context, 300.0, true, vec![]).unwrap();
        assert!(narrow.width() <= 225.0);
    }
}
