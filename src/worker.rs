use crate::data::{Signal, SignalSeries};
use crossbeam_channel::{Receiver, Sender};
use egui_plot::PlotPoint;
use std::f64::consts::PI;

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
        command_receiver: Receiver<Signal>,
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
                            let value = calculate_signal_value(&signal_series.signal, delta_t);

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
                    Ok(new_signal) => {
                        let response = format!("New signal added.");
                        if let Ok(mut all_series) = series.lock() {
                            all_series.push(SignalSeries {
                                signal: new_signal.clone(),
                                points: Vec::new(),
                                visible: true,
                            });
                        }
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

fn calculate_signal_value(signal: &Signal, t: f64) -> f64 {
    match signal {
        Signal::SineWave {
            amplitude,
            period,
            phase,
        } => amplitude * (2.0 * PI / period * t + phase).sin(),
        Signal::SquareWave {
            amplitude,
            period,
            duty_cycle,
        } => {
            let t_mod = t % period;
            if t_mod < period * duty_cycle {
                *amplitude
            } else {
                -(*amplitude)
            }
        }
        Signal::TriangleWave { amplitude, period } => {
            let t_mod = t % period;
            let normalized = t_mod / period;
            let value = if normalized < 0.5 {
                4.0 * normalized - 1.0
            } else {
                3.0 - 4.0 * normalized
            };
            amplitude * value
        }
        Signal::SawtoothWave { amplitude, period } => {
            let t_mod = t % period;
            let normalized = t_mod / period;
            amplitude * (2.0 * normalized - 1.0)
        }
        Signal::Constant { value } => *value,
    }
}
