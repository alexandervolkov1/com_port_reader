use crate::{
    components::plot_model::{PlotLine, PlotModel},
    data::{Sample, SeriesStore},
    utils::{current_time_f64, mark_for_timestamp},
};

use eframe::egui;

use egui_plot::{HoverPosition, Line, Plot, PlotBounds, PlotPoint, PlotPoints};

const WINDOW_SECONDS: f64 = 3600.0;
const DOWNSAMPLE_BUCKETS: usize = 2000;

pub fn show(ui: &mut egui::Ui, plot: &mut PlotModel, series_store: &SeriesStore) {
    let (min_x, max_x) = prepare_lines(plot, series_store);

    Plot::new("signals")
        .height(ui.available_height())
        .allow_drag(true)
        .allow_zoom(true)
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
            if plot.follow_latest {
                plot_ui.set_plot_bounds(PlotBounds::from_min_max([min_x, -120.0], [max_x, 120.0]));
            }

            for line in &plot.lines {
                plot_ui.line(
                    Line::new(line.name.clone(), PlotPoints::Owned(line.points.clone())).width(4.0),
                );
            }

            let response = plot_ui.response();

            if response.dragged() {
                plot.follow_latest = false;
                plot.last_plot_x = plot_ui.plot_bounds().min()[0];
            }

            if response.double_clicked() {
                plot.follow_latest = true;
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

        let (min_x, max_x) = if plot.follow_latest {
            if latest_x - first_x < WINDOW_SECONDS {
                (first_x, latest_x)
            } else {
                (latest_x - WINDOW_SECONDS, latest_x)
            }
        } else {
            (plot.last_plot_x, plot.last_plot_x + WINDOW_SECONDS)
        };

        plot.lines.resize_with(series.len(), PlotLine::default);

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

            let line = &mut plot.lines[prepared_count];

            line.name.clone_from(&signal_series.name);

            line.points.clear();

            downsample_min_max_into(visible_samples, DOWNSAMPLE_BUCKETS, &mut line.points);

            prepared_count += 1;
        }

        plot.lines.truncate(prepared_count);

        (min_x, max_x)
    })
}

fn downsample_min_max_into(samples: &[Sample], target_buckets: usize, output: &mut Vec<PlotPoint>) {
    if samples.len() <= target_buckets || target_buckets < 2 {
        output.extend(samples.iter().copied().map(plot_point_from_sample));

        return;
    }

    let bucket_size = samples.len() as f64 / target_buckets as f64;

    output.reserve(target_buckets * 2);

    let mut bucket_start = 0.0;

    while (bucket_start as usize) < samples.len() {
        let start = bucket_start as usize;

        let end = ((bucket_start + bucket_size) as usize).min(samples.len());

        if start >= end {
            break;
        }

        let bucket = &samples[start..end];

        let mut minimum = bucket[0];
        let mut maximum = bucket[0];

        for sample in bucket {
            if sample.value < minimum.value {
                minimum = *sample;
            }

            if sample.value > maximum.value {
                maximum = *sample;
            }
        }

        if minimum.timestamp < maximum.timestamp {
            output.push(plot_point_from_sample(minimum));

            output.push(plot_point_from_sample(maximum));
        } else {
            output.push(plot_point_from_sample(maximum));

            output.push(plot_point_from_sample(minimum));
        }

        bucket_start += bucket_size;
    }
}

fn plot_point_from_sample(sample: Sample) -> PlotPoint {
    PlotPoint {
        x: sample.timestamp,
        y: sample.value,
    }
}
