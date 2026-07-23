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

pub fn show(ui: &mut egui::Ui, plot: &mut PlotModel, series_store: &SeriesStore) {
    let (min_x, max_x) = prepare_lines(plot, series_store);
    let pane_id = plot.panes[0].id;

    Plot::new(("signals", pane_id))
        .height(ui.available_height())
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
                mark_for_timestamp(position.x),
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
            } else {
                plot_ui.set_auto_bounds([false, false]);

                let bounds = plot_ui.plot_bounds();

                plot.manual_x_bounds = Some((bounds.min()[0], bounds.max()[0]));
            }

            for line in &plot.panes[0].lines {
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

        let lines = &mut plot.panes[0].lines;

        lines.resize_with(series.len(), PlotLine::default);

        let mut prepared_count = 0;

        for signal_series in series {
            if !signal_series.visible {
                continue;
            }

            let start_idx = signal_series
                .samples
                .partition_point(|sample| sample.timestamp < min_x);

            let end_idx = signal_series
                .samples
                .partition_point(|sample| sample.timestamp <= max_x);

            let visible_samples = &signal_series.samples[start_idx..end_idx];

            let line = &mut lines[prepared_count];

            line.name.clone_from(&signal_series.name);

            line.points.clear();

            downsample_min_max_into(visible_samples, DOWNSAMPLE_BUCKETS, &mut line.points);

            prepared_count += 1;
        }

        lines.truncate(prepared_count);

        (min_x, max_x)
    })
}
