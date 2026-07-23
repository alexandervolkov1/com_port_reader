use crate::{
    components::{
        plot_downsampling::downsample_min_max_into,
        plot_model::{PlotLine, PlotModel},
    },
    data::SeriesStore,
    utils::{current_time_f64, mark_for_timestamp},
};

use eframe::egui;
use egui_plot::{HoverPosition, Line, Plot, PlotPoints};

const WINDOW_SECONDS: f64 = 3600.0;
const DOWNSAMPLE_BUCKETS: usize = 2000;
const MIN_PANE_HEIGHT: f32 = 80.0;

pub fn show(ui: &mut egui::Ui, plot: &mut PlotModel, series_store: &SeriesStore) {
    let (min_x, max_x) = prepare_lines(plot, series_store);

    let pane_count = plot.panes.len();

    let spacing = ui.spacing().item_spacing.y;

    let controls_height = ui.spacing().interact_size.y;

    let reserved_height = controls_height + spacing * pane_count as f32;

    let pane_height =
        ((ui.available_height() - reserved_height) / pane_count as f32).max(MIN_PANE_HEIGHT);

    for pane_index in 0..pane_count {
        show_pane(ui, plot, pane_index, min_x, max_x, pane_height);
    }

    ui.horizontal(|ui| {
        if ui.button("Add plot").clicked() {
            plot.add_pane();
        }

        let can_remove = plot.panes.len() > 1;

        if ui
            .add_enabled(can_remove, egui::Button::new("Remove last plot"))
            .clicked()
        {
            plot.remove_last_pane();
        }
    });
}

fn show_pane(
    ui: &mut egui::Ui,
    plot: &mut PlotModel,
    pane_index: usize,
    min_x: f64,
    max_x: f64,
    height: f32,
) {
    let pane_id = plot.panes[pane_index].id;

    Plot::new(("signals", pane_id))
        .height(height)
        .allow_drag(true)
        .allow_zoom(true)
        .auto_bounds([false, true])
        .x_grid_spacer(egui_plot::uniform_grid_spacer(|input| {
            let span = input.bounds.1 - input.bounds.0;

            if span < 600.0 {
                [60.0, 10.0, 1.0]
            } else if span < 1800.0 {
                [300.0, 60.0, 30.0]
            } else {
                [1800.0, 600.0, 60.0]
            }
        }))
        .x_axis_formatter(|mark, _range| mark_for_timestamp(mark.value))
        .label_formatter(|position| match position {
            HoverPosition::NearDataPoint {
                plot_name,
                position,
                ..
            } if !plot_name.is_empty() => Some(format!(
                "{}\n{}, {:.1}",
                plot_name,
                mark_for_timestamp(position.x,),
                position.y,
            )),

            _ => None,
        })
        .show(ui, |plot_ui| {
            let response = plot_ui.response();

            let pointer_scrolled = response.hovered()
                && plot_ui.ctx().input(|input| {
                    input.smooth_scroll_delta != egui::Vec2::ZERO
                        || (input.zoom_delta() - 1.0).abs() > f32::EPSILON
                });

            let user_interacted = response.dragged() || pointer_scrolled;

            if response.double_clicked() {
                plot.follow_latest = true;
                plot.manual_x_bounds = None;

                plot_ui.set_auto_bounds([false, true]);

                plot_ui.set_plot_bounds_x(min_x..=max_x);
            } else if user_interacted {
                plot.follow_latest = false;

                plot_ui.set_auto_bounds([false, false]);

                let bounds = plot_ui.plot_bounds();

                plot.manual_x_bounds = Some((bounds.min()[0], bounds.max()[0]));
            } else if plot.follow_latest {
                plot_ui.set_auto_bounds([false, true]);

                plot_ui.set_plot_bounds_x(min_x..=max_x);
            } else if let Some((manual_min_x, manual_max_x)) = plot.manual_x_bounds {
                plot_ui.set_auto_bounds([false, false]);

                plot_ui.set_plot_bounds_x(manual_min_x..=manual_max_x);
            }

            for line in &plot.panes[pane_index].lines {
                plot_ui.line(
                    Line::new(line.name.clone(), PlotPoints::Borrowed(&line.points)).width(4.0),
                );
            }
        });
}

fn prepare_lines(plot: &mut PlotModel, series_store: &SeriesStore) -> (f64, f64) {
    series_store.with(|series| {
        let latest_x = series
            .iter()
            .filter_map(|series| series.samples.last())
            .map(|sample| sample.timestamp)
            .fold(current_time_f64(), f64::max);

        let first_x = series
            .iter()
            .filter_map(|series| series.samples.first())
            .map(|sample| sample.timestamp)
            .fold(latest_x, f64::min);

        let live_bounds = if latest_x - first_x < WINDOW_SECONDS {
            (first_x, latest_x)
        } else {
            (latest_x - WINDOW_SECONDS, latest_x)
        };

        let (min_x, max_x) = if plot.follow_latest {
            live_bounds
        } else {
            plot.manual_x_bounds.unwrap_or(live_bounds)
        };

        let default_pane_id = plot.panes[0].id;

        let series_panes = &plot.series_panes;

        for pane in &mut plot.panes {
            pane.lines.resize_with(series.len(), PlotLine::default);

            let mut prepared_count = 0;

            for signal_series in series {
                if !signal_series.visible {
                    continue;
                }

                let assigned_pane = series_panes
                    .get(&signal_series.id)
                    .copied()
                    .unwrap_or(default_pane_id);

                if assigned_pane != pane.id {
                    continue;
                }

                let start_idx = signal_series
                    .samples
                    .partition_point(|sample| sample.timestamp < min_x);

                let end_idx = signal_series
                    .samples
                    .partition_point(|sample| sample.timestamp <= max_x);

                let visible_samples = &signal_series.samples[start_idx..end_idx];

                let line = &mut pane.lines[prepared_count];

                line.name.clone_from(&signal_series.name);

                line.points.clear();

                downsample_min_max_into(visible_samples, DOWNSAMPLE_BUCKETS, &mut line.points);

                prepared_count += 1;
            }

            pane.lines.truncate(prepared_count);
        }

        (min_x, max_x)
    })
}
