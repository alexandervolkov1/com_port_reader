use crossbeam_channel::{Receiver, Sender};
use egui_plot::PlotPoint;
use std::f64::consts::PI;
use std::fmt::Display;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone)]
pub enum Signal {
    SineWave {
        amplitude: f64,
        period: f64,
        phase: f64,
    },
    SquareWave {
        amplitude: f64,
        period: f64,
        duty_cycle: f64,
    },
    TriangleWave {
        amplitude: f64,
        period: f64,
    },
    SawtoothWave {
        amplitude: f64,
        period: f64,
    },
    Constant {
        value: f64,
    },
}

impl Default for Signal {
    fn default() -> Self {
        Signal::SineWave {
            amplitude: (100.0),
            period: (50.0),
            phase: (0.0),
        }
    }
}

impl Signal {
    pub fn from_string(str: &str) -> Result<Self, String> {
        let tokens: Vec<&str> = str.trim().split_whitespace().collect();

        if tokens.is_empty() {
            return Err("Empty input".to_string());
        }

        match tokens[0].to_lowercase().as_str() {
            "sin" | "sine" => {
                let amplitude = parse_parameter(tokens.get(1), 100.0)?;
                let period = parse_parameter(tokens.get(2), 50.0)?;
                let phase = parse_parameter(tokens.get(3), 0.0)?;

                Ok(Signal::SineWave {
                    amplitude,
                    period,
                    phase,
                })
            }
            "square" | "sq" => {
                let amplitude = parse_parameter(tokens.get(1), 100.0)?;
                let period = parse_parameter(tokens.get(2), 1.0)?;
                let duty_cycle = parse_parameter(tokens.get(3), 0.5)?;

                if duty_cycle < 0.0 || duty_cycle > 1.0 {
                    return Err("Duty cycle must be between 0 and 1".to_string());
                }

                Ok(Signal::SquareWave {
                    amplitude,
                    period,
                    duty_cycle,
                })
            }
            "triangle" | "tri" => {
                let amplitude = parse_parameter(tokens.get(1), 100.0)?;
                let period = parse_parameter(tokens.get(2), 1.0)?;

                Ok(Signal::TriangleWave { amplitude, period })
            }
            "saw" | "sawtooth" => {
                let amplitude = parse_parameter(tokens.get(1), 100.0)?;
                let period = parse_parameter(tokens.get(2), 1.0)?;

                Ok(Signal::SawtoothWave { amplitude, period })
            }
            "const" | "constant" => {
                let value = parse_parameter(tokens.get(1), 0.0)?;

                Ok(Signal::Constant { value })
            }
            _ => Err(format!("Unknown signal type: {}", tokens[0])),
        }
    }
}

impl Display for Signal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Signal::SineWave {
                amplitude,
                period,
                phase,
            } => {
                write!(
                    f,
                    "SineWave(amp={}, period={}, phase={})",
                    amplitude, period, phase
                )
            }
            Signal::SquareWave {
                amplitude,
                period,
                duty_cycle,
            } => {
                write!(
                    f,
                    "SquareWave(amp={}, period={}, duty={})",
                    amplitude, period, duty_cycle
                )
            }
            Signal::TriangleWave { amplitude, period } => {
                write!(f, "TriangleWave(amp={}, period={})", amplitude, period)
            }
            Signal::SawtoothWave { amplitude, period } => {
                write!(f, "SawtoothWave(amp={}, period={})", amplitude, period)
            }
            Signal::Constant { value } => {
                write!(f, "Constant(val={})", value)
            }
        }
    }
}

pub struct Worker {
    handle: Option<JoinHandle<()>>,
    running: Arc<AtomicBool>,
    current_signal: Arc<Mutex<Signal>>,
}

impl Worker {
    pub fn new() -> Self {
        Self {
            handle: None,
            running: Arc::new(AtomicBool::new(false)),
            current_signal: Arc::new(Mutex::new(Signal::default())),
        }
    }

    pub fn start(
        &mut self,
        command_receiver: Receiver<Signal>,
        response_sender: Sender<String>,
        points: Arc<Mutex<Vec<PlotPoint>>>,
    ) {
        if self.handle.is_some() {
            return;
        }

        self.running.store(true, Ordering::Release);
        let running = self.running.clone();
        let current_signal = self.current_signal.clone();

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

                    let signal_value = {
                        let signal = current_signal.lock().unwrap();
                        calculate_signal_value(&*signal, delta_t)
                    };

                    if let Ok(mut points) = points.lock() {
                        points.push(PlotPoint {
                            x: timestamp,
                            y: signal_value,
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
                    Ok(new_signal) => {
                        if let Ok(mut signal) = current_signal.lock() {
                            *signal = new_signal.clone();
                            let response = format!("Signal changed to: {}", new_signal);

                            let _ = response_sender.send(response);
                        }
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

fn parse_parameter(param: Option<&&str>, default: f64) -> Result<f64, String> {
    match param {
        Some(s) => s
            .parse::<f64>()
            .map_err(|e| format!("Failed to parse number '{}': {}", s, e)),
        None => Ok(default),
    }
}
