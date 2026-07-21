mod command;
mod event;
mod handle;

pub use command::WorkerCommand;
pub use event::WorkerEvent;
pub use handle::{WorkerHandle, WorkerHandleError};

use crate::data::{Sample, SeriesStore};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const POLL_INTERVAL: Duration = Duration::from_millis(1000);

enum AcquisitionState {
    Stopped,

    Running {
        started_at: Instant,
        next_poll: Instant,
    },
}

pub struct Worker {
    thread: Option<JoinHandle<()>>,
    commands: WorkerHandle,
    running: Arc<AtomicBool>,
}

impl Worker {
    pub fn spawn(
        commands: WorkerHandle,
        command_receiver: Receiver<WorkerCommand>,
        event_sender: Sender<WorkerEvent>,
        series: SeriesStore,
    ) -> Self {
        let running = Arc::new(AtomicBool::new(false));
        let thread_running = running.clone();

        let thread = thread::spawn(move || {
            let mut state = AcquisitionState::Stopped;

            loop {
                let now = Instant::now();

                if let AcquisitionState::Running {
                    started_at,
                    next_poll,
                } = &mut state
                {
                    if now >= *next_poll {
                        let elapsed_seconds = started_at.elapsed().as_secs_f64();

                        let timestamp = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs_f64();

                        series.with_mut(|all_series| {
                            for signal_series in all_series {
                                let value = signal_series.signal.value_at(elapsed_seconds);

                                signal_series.samples.push(Sample::new(timestamp, value));
                            }
                        });

                        *next_poll += POLL_INTERVAL;

                        if Instant::now() > *next_poll + POLL_INTERVAL {
                            *next_poll = Instant::now() + POLL_INTERVAL;
                        }

                        continue;
                    }
                }

                let command_result = match &state {
                    AcquisitionState::Stopped => command_receiver
                        .recv()
                        .map_err(|_| RecvTimeoutError::Disconnected),

                    AcquisitionState::Running { next_poll, .. } => {
                        let timeout = next_poll.saturating_duration_since(now);

                        command_receiver.recv_timeout(timeout)
                    }
                };

                match command_result {
                    Ok(WorkerCommand::Start) => {
                        if matches!(state, AcquisitionState::Stopped) {
                            let started_at = Instant::now();

                            state = AcquisitionState::Running {
                                started_at,
                                next_poll: started_at + POLL_INTERVAL,
                            };

                            thread_running.store(true, Ordering::Release);
                        }
                    }

                    Ok(WorkerCommand::Stop) => {
                        state = AcquisitionState::Stopped;

                        thread_running.store(false, Ordering::Release);
                    }

                    Ok(WorkerCommand::AddSeries(new_series)) => {
                        let event = match series.add_series(new_series) {
                            Ok(id) => WorkerEvent::SeriesAdded(id),

                            Err(error) => WorkerEvent::SeriesAddFailed(error),
                        };

                        let _ = event_sender.send(event);
                    }

                    Ok(WorkerCommand::RemoveSeries(id)) => {
                        series.remove_series(id);
                    }

                    Ok(WorkerCommand::SetVisibility { id, visible }) => {
                        series.set_visibility(id, visible);
                    }

                    Ok(WorkerCommand::ClearSeries) => {
                        series.clear();
                    }

                    Ok(WorkerCommand::Shutdown) => {
                        break;
                    }

                    Err(RecvTimeoutError::Timeout) => {
                        // Наступил срок очередного опроса.
                    }

                    Err(RecvTimeoutError::Disconnected) => {
                        break;
                    }
                }
            }

            thread_running.store(false, Ordering::Release);
        });

        Self {
            thread: Some(thread),
            commands,
            running,
        }
    }

    pub fn start(&self) {
        let _ = self.commands.start();
    }

    pub fn stop(&self) {
        let _ = self.commands.stop();
    }

    pub fn clear_series(&self) {
        let _ = self.commands.clear_series();
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.commands.shutdown();

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
