use crossbeam_channel::{Receiver, Sender};
use egui_plot::PlotPoint;
use std::f64::consts::PI;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(100);

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

    pub fn start(
        &mut self,
        command_receiver: Receiver<String>,
        response_sender: Sender<String>,
        points: Arc<Mutex<Vec<PlotPoint>>>,
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
