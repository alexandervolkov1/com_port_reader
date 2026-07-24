mod command;
mod config;
mod event;
mod handle;

pub use command::WorkerCommand;
pub use config::WorkerConfig;
pub use event::WorkerEvent;
pub use handle::{WorkerHandle, WorkerHandleError};

use crate::sample_sink::{CsvSampleSink, NullSampleSink, SampleSink, SampleSinkError};
use crate::utils::current_time_f64;
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
use std::time::Instant;

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
    sample_sink_active: Arc<AtomicBool>,
}

impl Worker {
    pub fn spawn(
        commands: WorkerHandle,
        command_receiver: Receiver<WorkerCommand>,
        event_sender: Sender<WorkerEvent>,
        series: SeriesStore,
        mut source: Box<dyn AcquisitionSource>,
        mut sink: Box<dyn SampleSink>,
        config: WorkerConfig,
    ) -> Self {
        let running = Arc::new(AtomicBool::new(false));
        let sample_sink_active = Arc::new(AtomicBool::new(false));
        let thread_sample_sink_active = sample_sink_active.clone();
        let thread_running = running.clone();
        let poll_interval = config.poll_interval();

        let thread = thread::spawn(move || {
            let mut state = AcquisitionState::Stopped;
            let mut sample_batch: Vec<SeriesSample> = Vec::new();
            loop {
                let now = Instant::now();

                let mut poll_completed = false;

                let mut acquisition_error: Option<AcquisitionError> = None;
                let mut sink_error: Option<SampleSinkError> = None;

                if let AcquisitionState::Running {
                    started_at,
                    next_poll,
                } = &mut state
                    && now >= *next_poll
                {
                    let elapsed_seconds = started_at.elapsed().as_secs_f64();

                    let timestamp = current_time_f64();

                    sample_batch.clear();

                    let series_metadata = series.metadata();

                    let result = source.sample(
                        &series_metadata,
                        timestamp,
                        elapsed_seconds,
                        &mut sample_batch,
                    );

                    let result = match result {
                        Ok(()) => series.with_mut(|all_series| {
                            append_series_samples(all_series, &sample_batch)
                        }),

                        Err(error) => Err(error),
                    };

                    match result {
                        Ok(()) => match sink.write_batch(&sample_batch) {
                            Ok(()) => {
                                *next_poll += poll_interval;

                                if Instant::now() > *next_poll + poll_interval {
                                    *next_poll = Instant::now() + poll_interval;
                                }

                                poll_completed = true;
                            }

                            Err(error) => {
                                sink_error = Some(error);
                            }
                        },

                        Err(error) => {
                            acquisition_error = Some(error);
                        }
                    }
                }

                if let Some(mut error) = acquisition_error {
                    state = AcquisitionState::Stopped;

                    thread_running.store(false, Ordering::Release);

                    if let Err(stop_error) = source.stop() {
                        error = format!(
                            "{error}; additionally failed to stop source: \
                             {stop_error}",
                        )
                        .into();
                    }

                    if let Err(flush_error) = sink.flush() {
                        error = format!(
                            "{error}; additionally failed to flush sink: \
                             {flush_error}",
                        )
                        .into();
                    }

                    let _ = event_sender.send(WorkerEvent::AcquisitionFailed(error));

                    continue;
                }

                if let Some(mut error) = sink_error {
                    state = AcquisitionState::Stopped;

                    thread_running.store(false, Ordering::Release);

                    if let Err(stop_error) = source.stop() {
                        error = format!(
                            "{error}; additionally failed to stop source: \
                             {stop_error}",
                        )
                        .into();
                    }

                    if let Err(flush_error) = sink.flush() {
                        error = format!(
                            "{error}; additionally failed to flush sink: \
                             {flush_error}",
                        )
                        .into();
                    }

                    let _ = event_sender.send(WorkerEvent::SampleSinkFailed(error));
                    sink = Box::new(NullSampleSink::new());

                    thread_sample_sink_active.store(false, Ordering::Release);

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

                            if let Err(error) = sink.flush() {
                                let _ = event_sender.send(WorkerEvent::SampleSinkFailed(error));

                                sink = Box::new(NullSampleSink::new());

                                thread_sample_sink_active.store(false, Ordering::Release);
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
                        if series.remove_series(id) {
                            let _ = event_sender.send(WorkerEvent::SeriesRemoved(id));
                        }
                    }

                    Ok(WorkerCommand::SetVisibility { id, visible }) => {
                        series.set_visibility(id, visible);
                    }

                    Ok(WorkerCommand::ClearSeries) => {
                        series.clear();
                    }

                    Ok(WorkerCommand::StartCsvRecording(path)) => {
                        if let Err(error) = sink.flush() {
                            let _ = event_sender.send(WorkerEvent::SampleSinkFailed(error));
                        }

                        sink = Box::new(NullSampleSink::new());

                        thread_sample_sink_active.store(false, Ordering::Release);

                        match CsvSampleSink::create(path) {
                            Ok(csv_sink) => {
                                sink = Box::new(csv_sink);

                                thread_sample_sink_active.store(true, Ordering::Release);
                            }

                            Err(error) => {
                                let _ = event_sender.send(WorkerEvent::SampleSinkFailed(error));
                            }
                        }
                    }

                    Ok(WorkerCommand::StopRecording) => {
                        if let Err(error) = sink.flush() {
                            let _ = event_sender.send(WorkerEvent::SampleSinkFailed(error));
                        }

                        sink = Box::new(NullSampleSink::new());

                        thread_sample_sink_active.store(false, Ordering::Release);
                    }

                    Ok(WorkerCommand::Shutdown) => {
                        if matches!(state, AcquisitionState::Running { .. }) {
                            let _ = source.stop();
                        }

                        let _ = sink.flush();

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
                    Ok(WorkerCommand::TestSerialPort(config)) => {
                        let port_name = config.port_name().to_owned();

                        let event = match config.open() {
                            Ok(port) => {
                                drop(port);

                                WorkerEvent::SerialPortTestSucceeded(port_name)
                            }

                            Err(error) => WorkerEvent::SerialPortTestFailed { port_name, error },
                        };

                        let _ = event_sender.send(event);
                    }

                    Ok(WorkerCommand::TestSerialCommand { config, command }) => {
                        let port_name = config.port_name().to_owned();

                        let result = config
                            .open()
                            .and_then(|mut connection| connection.request_f64(&command));

                        let event = match result {
                            Ok(value) => WorkerEvent::SerialCommandSucceeded {
                                port_name,
                                command,
                                value,
                            },

                            Err(error) => WorkerEvent::SerialCommandFailed {
                                port_name,
                                command,
                                error,
                            },
                        };

                        let _ = event_sender.send(event);
                    }

                    Err(RecvTimeoutError::Timeout) => {}

                    Err(RecvTimeoutError::Disconnected) => {
                        break;
                    }
                }
            }

            thread_running.store(false, Ordering::Release);
            thread_sample_sink_active.store(false, Ordering::Release);
        });

        Self {
            thread: Some(thread),
            commands,
            running,
            sample_sink_active,
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

    pub fn start_csv_recording(&self, path: std::path::PathBuf) -> Result<(), WorkerHandleError> {
        self.commands.start_csv_recording(path)
    }

    pub fn stop_recording(&self) -> Result<(), WorkerHandleError> {
        self.commands.stop_recording()
    }

    pub fn is_recording(&self) -> bool {
        self.sample_sink_active.load(Ordering::Acquire)
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
