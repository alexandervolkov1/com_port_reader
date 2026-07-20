mod command;

pub use command::WorkerCommand;

use crate::data::SignalSeries;
use crossbeam_channel::{Receiver, Sender};
use egui_plot::PlotPoint;

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const POLL_INTERVAL: Duration = Duration::from_millis(1000);

pub struct Worker {
    handle: Option<JoinHandle<()>>,
    running: Arc<AtomicBool>,
}

impl Worker {
    pub fn new() -> Self {
        Self {
            handle: None,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn start(
        &mut self,
        command_receiver: Receiver<WorkerCommand>,
        response_sender: Sender<String>,
        series: Arc<Mutex<Vec<SignalSeries>>>,
    ) {
        if self.handle.is_some() {
            return;
        }

        self.running.store(true, Ordering::Release);
        let running = self.running.clone();

        self.handle = Some(thread::spawn(move || {
            let start_time = Instant::now();
            let mut next_poll = start_time + POLL_INTERVAL;

            while running.load(Ordering::Acquire) {
                let now = Instant::now();

                if now >= next_poll {
                    let delta_t = start_time.elapsed().as_secs_f64();

                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs_f64();

                    if let Ok(mut all_series) = series.lock() {
                        for signal_series in all_series.iter_mut() {
                            let value = signal_series.signal.value_at(delta_t);

                            signal_series.points.push(PlotPoint {
                                x: timestamp,
                                y: value,
                            });
                        }
                    }
                    next_poll += POLL_INTERVAL;

                    if Instant::now() > next_poll + POLL_INTERVAL {
                        next_poll = Instant::now() + POLL_INTERVAL;
                    }
                    continue;
                }

                let timeout = next_poll.saturating_duration_since(now);

                match command_receiver.recv_timeout(timeout) {
                    Ok(WorkerCommand::AddSignal(signal)) => {
                        if let Ok(mut all_series) = series.lock() {
                            all_series.push(SignalSeries {
                                signal,
                                points: Vec::new(),
                                visible: true,
                            });
                        }

                        let _ = response_sender.send("New signal added.".to_owned());
                    }

                    Err(_) => {}
                }
            }
        }));
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Release);

        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}
