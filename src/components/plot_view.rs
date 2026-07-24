use crate::{
    components::{
        plot_downsampling::downsample_min_max_into,
        plot_model::{PlotLine, PlotModel},
    },
    data::SeriesStore,
    utils::{current_time_f64, mark_for_timestamp},
};

use eframe::egui;
use egui_plot::{AxisHints, GridInput, GridMark, HoverPosition, Line, Plot, PlotPoints};

const WINDOW_SECONDS: f64 = 3600.0;
const DOWNSAMPLE_BUCKETS: usize = 2000;
const MIN_PANE_HEIGHT: f32 = 80.0;

const Y_AXIS_MIN_WIDTH: f32 = 50.0;
const Y_LABEL_MIN_SPACING: f32 = 14.0;
const Y_LABEL_FULL_SPACING: f32 = 20.0;
const X_LABEL_LEFT_MARGIN: f64 = 0.035;

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
    let auto_y = plot.panes[pane_index].auto_y;
    let pane_is_empty = plot.panes[pane_index].lines.is_empty();

    let requested_x_bounds = if plot.follow_latest {
        (min_x, max_x)
    } else {
        plot.manual_x_bounds.unwrap_or((min_x, max_x))
    };

    let plot_widget = Plot::new(("signals", pane_id))
        .height(height)
        .custom_y_axes(vec![
            AxisHints::new_y()
                .min_thickness(Y_AXIS_MIN_WIDTH)
                .label_spacing(egui::Rangef::new(Y_LABEL_MIN_SPACING, Y_LABEL_FULL_SPACING)),
        ])
        .y_grid_spacer(y_grid_marks)
        .y_axis_formatter(|mark, _range| format_y_mark(mark))
        .allow_drag(true)
        .allow_zoom(true)
        .allow_double_click_reset(false)
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
        .x_axis_formatter(format_x_mark)
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
        });

    let plot_widget = if pane_is_empty {
        plot_widget.default_y_bounds(-1.0, 1.0)
    } else {
        plot_widget
    };

    let plot_response = plot_widget.show(ui, |plot_ui| {
        plot_ui.set_auto_bounds([false, auto_y]);

        plot_ui.set_plot_bounds_x(requested_x_bounds.0..=requested_x_bounds.1);

        for line in &plot.panes[pane_index].lines {
            plot_ui
                .line(Line::new(line.name.clone(), PlotPoints::Borrowed(&line.points)).width(4.0));
        }
    });

    let response = &plot_response.response;
    let actual_bounds = plot_response.transform.bounds();

    let actual_x_bounds = (actual_bounds.min()[0], actual_bounds.max()[0]);

    let pointer_navigation = response.contains_pointer()
        && ui.ctx().input(|input| {
            input.smooth_scroll_delta != egui::Vec2::ZERO
                || input.zoom_delta_2d() != egui::Vec2::splat(1.0)
        });

    let x_bounds_changed = x_bounds_differ(requested_x_bounds, actual_x_bounds);

    let pointer_down = ui.ctx().input(|input| input.pointer.any_down());

    let user_interacted = response.dragged()
        || response.drag_stopped()
        || pointer_navigation
        || (pointer_down && x_bounds_changed);

    if response.double_clicked() {
        plot.follow_latest = true;
        plot.manual_x_bounds = None;

        for pane in &mut plot.panes {
            pane.auto_y = true;
        }
    } else if user_interacted {
        plot.follow_latest = false;
        plot.manual_x_bounds = Some(actual_x_bounds);
        plot.panes[pane_index].auto_y = false;
    }
}

fn y_grid_marks(input: GridInput) -> Vec<GridMark> {
    const EGUI_PLOT_MIN_GRID_SPACING: f64 = 8.0;

    if !input.base_step_size.is_finite()
        || input.base_step_size <= 0.0
        || !input.bounds.0.is_finite()
        || !input.bounds.1.is_finite()
    {
        return Vec::new();
    }

    let minimum_step =
        input.base_step_size * f64::from(Y_LABEL_FULL_SPACING) / EGUI_PLOT_MIN_GRID_SPACING;

    let step = nice_y_step(minimum_step);

    let first_index = (input.bounds.0 / step).ceil() as i64;

    let last_index = (input.bounds.1 / step).floor() as i64;

    if first_index > last_index {
        return Vec::new();
    }

    (first_index..=last_index)
        .map(|index| GridMark {
            value: index as f64 * step,
            step_size: step,
        })
        .collect()
}

fn nice_y_step(minimum_step: f64) -> f64 {
    let magnitude = 10_f64.powf(minimum_step.log10().floor());

    let normalized = minimum_step / magnitude;

    let factor = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };

    factor * magnitude
}

fn format_y_mark(mark: GridMark) -> String {
    let step = mark.step_size.abs();

    let value = if mark.value.abs() < step * 1.0e-9 {
        0.0
    } else {
        mark.value
    };

    if value == 0.0 {
        return "0".to_owned();
    }

    let decimals = if step >= 1.0 {
        0
    } else {
        (-step.log10()).ceil() as usize
    };

    if decimals <= 6 {
        format!("{value:.decimals$}")
    } else {
        format!("{value:.3e}")
    }
}

fn x_bounds_differ(expected: (f64, f64), actual: (f64, f64)) -> bool {
    let expected_span = (expected.1 - expected.0).abs();

    let tolerance = expected_span.max(1.0) * 1.0e-9;

    (expected.0 - actual.0).abs() > tolerance || (expected.1 - actual.1).abs() > tolerance
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

        let data_span = latest_x - first_x;

        let live_bounds = if data_span <= 0.0 {
            (latest_x - WINDOW_SECONDS, latest_x)
        } else if data_span < WINDOW_SECONDS {
            (first_x, latest_x)
        } else {
            (latest_x - WINDOW_SECONDS, latest_x)
        };

        let (min_x, max_x) = if plot.follow_latest {
            live_bounds
        } else {
            plot.manual_x_bounds.unwrap_or(live_bounds)
        };

        plot.series_panes.retain(|series_id, _| {
            series
                .iter()
                .any(|stored_series| stored_series.id == *series_id)
        });

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

fn format_x_mark(mark: GridMark, visible_range: &std::ops::RangeInclusive<f64>) -> String {
    let min_x = *visible_range.start();
    let max_x = *visible_range.end();

    let edge_margin = (max_x - min_x).abs() * X_LABEL_LEFT_MARGIN;

    if mark.value < min_x + edge_margin {
        String::new()
    } else {
        mark_for_timestamp(mark.value)
    }
}
