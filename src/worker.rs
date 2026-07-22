mod command;
mod config;
mod event;
mod handle;

pub use command::WorkerCommand;
pub use config::WorkerConfig;
pub use event::WorkerEvent;
pub use handle::{WorkerHandle, WorkerHandleError};

use crate::{
    acquisition::{AcquisitionError, AcquisitionSource},
    data::{SeriesSample, SeriesStore, SignalSeries},
};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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
        mut source: Box<dyn AcquisitionSource>,
        config: WorkerConfig,
    ) -> Self {
        let running = Arc::new(AtomicBool::new(false));
        let thread_running = running.clone();
        let poll_interval = config.poll_interval();

        let thread = thread::spawn(move || {
            let mut state = AcquisitionState::Stopped;
            let mut sample_batch: Vec<SeriesSample> = Vec::new();
            loop {
                let now = Instant::now();

                let mut poll_completed = false;
                let mut acquisition_error = None;

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

                        sample_batch.clear();

                        let result = series.with_mut(|all_series| {
                            source.sample(
                                all_series,
                                timestamp,
                                elapsed_seconds,
                                &mut sample_batch,
                            )?;

                            append_series_samples(all_series, &sample_batch)
                        });

                        match result {
                            Ok(()) => {
                                *next_poll += poll_interval;

                                if Instant::now() > *next_poll + poll_interval {
                                    *next_poll = Instant::now() + poll_interval;
                                }

                                poll_completed = true;
                            }

                            Err(error) => {
                                acquisition_error = Some(error);
                            }
                        }
                    }
                }

                if let Some(mut error) = acquisition_error {
                    state = AcquisitionState::Stopped;

                    thread_running.store(false, Ordering::Release);

                    if let Err(stop_error) = source.stop() {
                        error = format!(
                            "{error}; additionally failed to stop source: \
                             {stop_error}"
                        )
                        .into();
                    }

                    let _ = event_sender.send(WorkerEvent::AcquisitionFailed(error));

                    continue;
                }

                if poll_completed {
                    continue;
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
                            match source.start() {
                                Ok(()) => {
                                    let started_at = Instant::now();

                                    state = AcquisitionState::Running {
                                        started_at,
                                        next_poll: started_at + poll_interval,
                                    };

                                    thread_running.store(true, Ordering::Release);
                                }

                                Err(error) => {
                                    let _ = event_sender
                                        .send(WorkerEvent::AcquisitionStartFailed(error));
                                }
                            }
                        }
                    }

                    Ok(WorkerCommand::Stop) => {
                        if matches!(state, AcquisitionState::Running { .. }) {
                            state = AcquisitionState::Stopped;

                            thread_running.store(false, Ordering::Release);

                            if let Err(error) = source.stop() {
                                let _ =
                                    event_sender.send(WorkerEvent::AcquisitionStopFailed(error));
                            }
                        }
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
                        if matches!(state, AcquisitionState::Running { .. }) {
                            let _ = source.stop();
                        }

                        break;
                    }

                    Ok(WorkerCommand::RemoveSeriesByName(name)) => {
                        let event = match series.remove_series_by_name(&name) {
                            Some(id) => WorkerEvent::SeriesRemoved(id),
                            None => WorkerEvent::SeriesNotFound(name),
                        };

                        let _ = event_sender.send(event);
                    }

                    Ok(WorkerCommand::RenameSeries {
                        current_name,
                        new_name,
                    }) => {
                        let event = match series.rename_series(&current_name, &new_name) {
                            Ok(id) => WorkerEvent::SeriesRenamed { id, name: new_name },

                            Err(error) => WorkerEvent::SeriesRenameFailed(error),
                        };

                        let _ = event_sender.send(event);
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

fn append_series_samples(
    series: &mut [SignalSeries],
    samples: &[SeriesSample],
) -> Result<(), AcquisitionError> {
    for series_sample in samples {
        if !series
            .iter()
            .any(|series| series.id == series_sample.series_id)
        {
            return Err(format!(
                "Acquisition source returned a sample for \
                 unknown series {}",
                series_sample.series_id,
            )
            .into());
        }
    }

    for series_sample in samples {
        let target = series
            .iter_mut()
            .find(|series| series.id == series_sample.series_id)
            .expect("series IDs were validated above");

        target.samples.push(series_sample.sample);
    }

    Ok(())
}
