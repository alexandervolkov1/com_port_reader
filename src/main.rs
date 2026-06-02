#![cfg_attr(windows, windows_subsystem = "windows")]

use crossbeam_channel::{Receiver, Sender};
use eframe::egui::{self};
use egui_plot::{Line, Plot, PlotBounds, PlotPoint, PlotPoints};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

mod worker;

const WINDOW_SECONDS: f64 = 3600.0;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "COM Port Reader",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(MyApp::new()))),
    )
}

struct MyApp {
    points: Arc<Mutex<Vec<PlotPoint>>>,
    worker: worker::Worker,

    command_sender: Sender<String>,
    command_receiver: Receiver<String>,

    response_sender: Sender<String>,
    response_receiver: Receiver<String>,

    command_buffer: String,
    last_response: String,

    follow_latest: bool,
    last_plot_x: f64,
}

impl MyApp {
    fn new() -> Self {
        let (command_sender, command_receiver) = crossbeam_channel::bounded(32);
        let (response_sender, response_receiver) = crossbeam_channel::bounded(32);
        Self {
            points: Arc::new(Mutex::new(Vec::new())),
            worker: worker::Worker::new(),

            command_sender,
            command_receiver,
            response_sender,
            response_receiver,

            command_buffer: String::new(),
            last_response: String::new(),

            follow_latest: true,
            last_plot_x: 0.0,
        }
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Ok(response) = self.response_receiver.try_recv() {
            self.last_response = response
        }

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Start").clicked() {
                    self.worker.start(
                        self.command_receiver.clone(),
                        self.response_sender.clone(),
                        self.points.clone(),
                    );
                }

                if ui.button("Stop").clicked() {
                    self.worker.stop();
                }

                if ui.button("Clear").clicked() {
                    if let Ok(mut points) = self.points.lock() {
                        points.clear();
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label("Command:");
                let response = ui.text_edit_singleline(&mut self.command_buffer);

                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if !self.command_buffer.is_empty() {
                        let _ = self.command_sender.send(self.command_buffer.clone());
                        self.command_buffer.clear();
                    }
                    response.request_focus();
                }
            });

            ui.label(format!("{}", self.last_response));

            if let Ok(points) = self.points.lock() {
                ui.label(format!("{}", points.len()));

                let first_x = points.first().map(|p| p.x).unwrap_or(current_time_f64());
                let latest_x = points
                    .last()
                    .map(|p| p.x)
                    .unwrap_or(current_time_f64() + 60.0);

                let (min_x, max_x) = if self.follow_latest {
                    if latest_x - first_x < WINDOW_SECONDS {
                        (first_x, latest_x)
                    } else {
                        (latest_x - WINDOW_SECONDS, latest_x)
                    }
                } else {
                    (self.last_plot_x, self.last_plot_x + WINDOW_SECONDS)
                };

                let start_idx = points.partition_point(|p| p.x < min_x);
                let end_idx = points.partition_point(|p| p.x <= max_x);

                let visible: Vec<PlotPoint> = points[start_idx..end_idx].to_vec();

                drop(points);

                let downsampled = downsample_min_max(&visible, 2000);

                Plot::new("sinus")
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
                    .label_formatter(|_s, val| {
                        format!("{}\n{:.1}", mark_for_timestamp(val.x), val.y)
                    })
                    .show(ui, |plot_ui| {
                        if self.follow_latest {
                            plot_ui.set_plot_bounds(PlotBounds::from_min_max(
                                [min_x, -120.0],
                                [max_x, 120.0],
                            ));
                        }

                        plot_ui.line(Line::new("sinus", PlotPoints::Owned(downsampled)).width(4.0));

                        self.last_plot_x = plot_ui.plot_bounds().min()[0];

                        let response = plot_ui.response();

                        if response.dragged() {
                            self.follow_latest = false;
                        }

                        if response.double_clicked() {
                            self.follow_latest = true;
                        }
                    });
            }
        });

        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }
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

fn mark_for_timestamp(timestamp: f64) -> String {
    chrono::DateTime::from_timestamp(timestamp as i64, 0)
        .unwrap()
        .with_timezone(&chrono::Local)
        .format("%H:%M:%S")
        .to_string()
}

fn current_time_f64() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}
