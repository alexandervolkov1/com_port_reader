#![cfg_attr(windows, windows_subsystem = "windows")]

use crossbeam_channel::{Receiver, Sender, bounded};
use eframe::egui::{self};
use egui_plot::{Line, Plot, PlotBounds, PlotPoint, PlotPoints};
use std::f64::consts::PI;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const WINDOW_SECONDS: f64 = 3600.0;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "COM Port Reader",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(MyApp::new()))),
    )
}

struct MyApp {
    command_buffer: String,

    points: Arc<Mutex<Vec<PlotPoint>>>,

    command_sender: Sender<String>,
    command_receiver: Receiver<String>,

    response_sender: Sender<String>,
    response_receiver: Receiver<String>,

    last_response: String,

    follow_latest: bool,
    last_plot_x: f64,

    worker: Option<JoinHandle<()>>,
    running: Arc<AtomicBool>,
}

impl MyApp {
    fn new() -> Self {
        let (command_sender, command_receiver) = bounded(32);
        let (response_sender, response_receiver) = bounded(32);
        Self {
            command_buffer: String::new(),

            points: Arc::new(Mutex::new(Vec::new())),

            command_sender,
            command_receiver,

            response_sender,
            response_receiver,

            last_response: String::new(),

            follow_latest: true,
            last_plot_x: 0.0,

            worker: None,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    fn start_worker(&mut self) {
        if self.worker.is_some() {
            return;
        }

        self.running.store(true, Ordering::Release);

        let command_receiver = self.command_receiver.clone();
        let response_sender = self.response_sender.clone();

        let running = self.running.clone();
        let points = self.points.clone();

        self.worker = Some(thread::spawn(move || {
            const POLL_INTERVAL: Duration = Duration::from_millis(100);
            let start_time = Instant::now();
            let mut next_poll = Instant::now() + POLL_INTERVAL;

            while running.load(Ordering::Acquire) {
                let now = Instant::now();

                if now >= next_poll {
                    let delta_t = start_time.elapsed().as_secs_f64();

                    let sinus_sum: f64 = (1..=10_000)
                        .step_by(2)
                        .map(|i| {
                            let i = i as f64;
                            4.0 * 100.0 / PI / i * (2.0 * PI * i * delta_t / 400.0).sin()
                        })
                        .sum();

                    if let Ok(mut points) = points.lock() {
                        points.push(PlotPoint {
                            x: delta_t,
                            y: sinus_sum,
                        });
                    }
                    next_poll += POLL_INTERVAL;

                    if Instant::now() > next_poll + POLL_INTERVAL {
                        next_poll = Instant::now() + POLL_INTERVAL;
                    }
                    continue;
                }

                let timeout = next_poll.saturating_duration_since(now);

                match command_receiver.recv_timeout(timeout) {
                    Ok(command) => {
                        let response = format!("You send: {}", command);
                        let _ = response_sender.send(response);
                    }

                    Err(_) => {}
                }
            }
        }));
    }

    fn stop_worker(&mut self) {
        self.running.store(false, Ordering::Release);

        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for MyApp {
    fn drop(&mut self) {
        self.stop_worker();
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
                    self.start_worker();
                }

                if ui.button("Stop").clicked() {
                    self.stop_worker();
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

                let latest_x = points.last().map(|p| p.x).unwrap_or(0.0);

                let (min_x, max_x) = if self.follow_latest {
                    if latest_x < WINDOW_SECONDS {
                        (0.0, latest_x.max(1.0))
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
                    .show(ui, |plot_ui| {
                        if self.follow_latest {
                            plot_ui.set_plot_bounds(PlotBounds::from_min_max(
                                [min_x, -120.0],
                                [max_x, 120.0],
                            ));
                        }

                        plot_ui.line(Line::new("sinus", PlotPoints::Owned(downsampled)));

                        let bounds = plot_ui.plot_bounds();

                        self.last_plot_x = bounds.min()[0];

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
