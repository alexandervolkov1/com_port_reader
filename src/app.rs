use crossbeam_channel::{Receiver, Sender};
use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{
    data::{Signal, SignalSeries},
    ui::plot::show_plot,
    worker::Worker,
};

pub struct MyApp {
    series: Arc<Mutex<Vec<SignalSeries>>>,
    worker: Worker,

    command_sender: Sender<Signal>,
    command_receiver: Receiver<Signal>,

    response_sender: Sender<String>,
    response_receiver: Receiver<String>,

    command_buffer: String,
    last_response: String,

    follow_latest: bool,
    last_plot_x: f64,
}

impl MyApp {
    pub fn new() -> Self {
        let (command_sender, command_receiver) = crossbeam_channel::bounded(32);
        let (response_sender, response_receiver) = crossbeam_channel::bounded(32);
        Self {
            series: Arc::new(Mutex::new(Vec::new())),
            worker: Worker::new(),

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
                        self.series.clone(),
                    );
                }

                if ui.button("Stop").clicked() {
                    self.worker.stop();
                }

                if ui.button("Clear").clicked() {
                    if let Ok(mut series) = self.series.lock() {
                        series.clear();
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label("Command:");
                let response = ui.text_edit_singleline(&mut self.command_buffer);

                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    match Signal::from_string(&self.command_buffer) {
                        Ok(signal) => {
                            let _ = self.command_sender.send(signal);
                        }
                        Err(e) => {
                            self.last_response = e;
                        }
                    };
                    self.command_buffer.clear();
                    response.request_focus();
                }
            });

            ui.label(format!("{}", self.last_response));

            if let Ok(series) = self.series.lock() {
                show_plot(
                    ui,
                    series.as_slice(),
                    &mut self.follow_latest,
                    &mut self.last_plot_x,
                );
            }
        });

        ui.ctx().request_repaint_after(Duration::from_millis(33));
    }
}
