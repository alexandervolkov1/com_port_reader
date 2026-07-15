use crate::{
    data::SignalSeries,
    utils::{current_time_f64, mark_for_timestamp},
};
use eframe::egui;
use egui_plot::{Line, Plot, PlotBounds, PlotPoint, PlotPoints};

const WINDOW_SECONDS: f64 = 3600.0;

pub fn show_plot(
    ui: &mut egui::Ui,
    series: &[SignalSeries],
    follow_latest: &mut bool,
    last_plot_x: &mut f64,
) {
    let latest_x = series
        .iter()
        .filter_map(|s| s.points.last())
        .map(|p| p.x)
        .fold(current_time_f64(), f64::max);

    let first_x = series
        .iter()
        .filter_map(|s| s.points.first())
        .map(|p| p.x)
        .fold(latest_x, f64::min);

    let (min_x, max_x) = if *follow_latest {
        if latest_x - first_x < WINDOW_SECONDS {
            (first_x, latest_x)
        } else {
            (latest_x - WINDOW_SECONDS, latest_x)
        }
    } else {
        (*last_plot_x, *last_plot_x + WINDOW_SECONDS)
    };

    Plot::new("signals")
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
        .label_formatter(|_s, val| format!("{}\n{:.1}", mark_for_timestamp(val.x), val.y))
        .show(ui, |plot_ui| {
            if *follow_latest {
                plot_ui.set_plot_bounds(PlotBounds::from_min_max([min_x, -120.0], [max_x, 120.0]));
            }

            for (idx, siignal_series) in series.iter().enumerate() {
                let start_idx = siignal_series.points.partition_point(|p| p.x < min_x);
                let end_idx = siignal_series.points.partition_point(|p| p.x <= max_x);

                let visible = &siignal_series.points[start_idx..end_idx];

                let downsampled = downsample_min_max(visible, 2000);

                plot_ui.line(
                    Line::new(format!("signal {}", idx), PlotPoints::Owned(downsampled)).width(4.0),
                );
            }

            *last_plot_x = plot_ui.plot_bounds().min()[0];

            let response = plot_ui.response();

            if response.dragged() {
                *follow_latest = false;
            }

            if response.double_clicked() {
                *follow_latest = true;
            }
        });
}

fn downsample_min_max(points: &[PlotPoint], target_points: usize) -> Vec<PlotPoint> {
    if points.len() <= target_points || target_points < 2 {
        return points.to_vec();
    }

    let bucket_size = points.len() as f64 / target_points as f64;

    let mut result = Vec::with_capacity(target_points * 2);

    let mut bucket_start = 0.0;

    while (bucket_start as usize) < points.len() {
        let start = bucket_start as usize;
        let end = ((bucket_start + bucket_size) as usize).min(points.len());

        if start >= end {
            break;
        }

        let slice = &points[start..end];

        let mut min = slice[0];
        let mut max = slice[0];

        for p in slice {
            if p.y < min.y {
                min = *p;
            }

            if p.y > max.y {
                max = *p;
            }
        }

        if min.x < max.x {
            result.push(min);
            result.push(max);
        } else {
            result.push(max);
            result.push(min);
        }

        bucket_start += bucket_size;
    }

    result
}
