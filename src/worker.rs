use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};

use crate::{
    acquisition::{
        AcquisitionError, AcquisitionSource, InstrumentReadResult, SeriesAcquisitionFailure,
        VirtualInstrumentDescribeResult,
    },
    connection::ConnectionId,
    data::{Series, SeriesId, SeriesMetadata, SeriesSample, SeriesStore},
    instrument::{InstrumentReadRequest, InstrumentWriteRequest},
    sample_sink::{CsvSampleSink, NullSampleSink, SampleSink, SampleSinkError},
    serial_connection::SerialConnectionError,
};

mod command;
mod config;
mod connections;
mod event;
mod handle;
mod serial;

pub use command::{ConnectionCommand, WorkerCommand};
pub use config::WorkerConfig;
pub use connections::{ConnectionWorkers, ConnectionWorkersError};
pub use event::WorkerEvent;
pub use handle::{WorkerHandle, WorkerHandleError};
pub use serial::{SpawnedConnectionWorker, spawn_serial_connection_worker};

enum AcquisitionState {
    Stopped,
    Running { next_poll: Instant },
}

#[derive(Clone, Copy, Debug)]
struct SeriesSchedule {
    interval: Duration,
    next_poll: Instant,
    consecutive_failures: u64,
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
        let connection_id = commands.connection_id();

        let running = Arc::new(AtomicBool::new(false));

        let sample_sink_active = Arc::new(AtomicBool::new(false));

        let thread_sample_sink_active = sample_sink_active.clone();

        let thread_running = running.clone();

        let initial_default_poll_interval = config.poll_interval();

        let thread = thread::spawn(move || {
            let mut default_poll_interval = initial_default_poll_interval;

            let mut series_schedules: HashMap<SeriesId, SeriesSchedule> = HashMap::new();

            let mut state = AcquisitionState::Stopped;

            let mut sample_batch: Vec<SeriesSample> = Vec::new();

            let mut sample_failures: Vec<SeriesAcquisitionFailure> = Vec::new();

            let mut pending_command: Option<WorkerCommand> = None;

            loop {
                let now = Instant::now();

                let series_metadata = if matches!(&state, AcquisitionState::Running { .. }) {
                    series.metadata_for_connection(connection_id)
                } else {
                    Vec::new()
                };

                if let AcquisitionState::Running { next_poll } = &mut state {
                    synchronize_series_schedules(
                        &mut series_schedules,
                        &series_metadata,
                        default_poll_interval,
                        now,
                    );

                    *next_poll =
                        next_series_poll(&series_schedules).unwrap_or(now + default_poll_interval);
                }

                let mut poll_completed = false;

                let mut acquisition_error: Option<AcquisitionError> = None;

                let mut sink_error: Option<SampleSinkError> = None;

                if let AcquisitionState::Running { next_poll } = &mut state
                    && now >= *next_poll
                {
                    let due_series = due_series_metadata(&series_metadata, &series_schedules, now);

                    sample_batch.clear();
                    sample_failures.clear();

                    source.sample(&due_series, &mut sample_batch, &mut sample_failures);

                    update_series_health(
                        &mut series_schedules,
                        &due_series,
                        &sample_failures,
                        &event_sender,
                    );

                    let result = series
                        .with_mut(|all_series| append_series_samples(all_series, &sample_batch));

                    let result = match result {
                        Ok(()) => series.with_mut(|all_series| {
                            append_series_samples(all_series, &sample_batch)
                        }),

                        Err(error) => Err(error),
                    };

                    match result {
                        Ok(()) => match sink.write_batch(&sample_batch, &series_metadata) {
                            Ok(()) => {
                                let completed_at = Instant::now();

                                advance_series_schedules(
                                    &mut series_schedules,
                                    &due_series,
                                    completed_at,
                                );

                                *next_poll = next_series_poll(&series_schedules)
                                    .unwrap_or(completed_at + default_poll_interval);

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

                    series_schedules.clear();

                    thread_running.store(false, Ordering::Release);

                    if let Err(stop_error) = source.stop() {
                        error = format!(
                            "{error}; additionally \
                             failed to stop source: \
                             {stop_error}",
                        )
                        .into();
                    }

                    if let Err(flush_error) = sink.flush() {
                        error = format!(
                            "{error}; additionally \
                             failed to flush sink: \
                             {flush_error}",
                        )
                        .into();
                    }

                    let _ = event_sender.send(WorkerEvent::AcquisitionFailed(error));

                    continue;
                }

                if let Some(mut error) = sink_error {
                    state = AcquisitionState::Stopped;

                    series_schedules.clear();

                    thread_running.store(false, Ordering::Release);

                    if let Err(stop_error) = source.stop() {
                        error = format!(
                            "{error}; additionally \
                             failed to stop source: \
                             {stop_error}",
                        )
                        .into();
                    }

                    if let Err(flush_error) = sink.flush() {
                        error = format!(
                            "{error}; additionally \
                             failed to flush sink: \
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

                let command_result = if let Some(command) = pending_command.take() {
                    Ok(command)
                } else {
                    match &state {
                        AcquisitionState::Stopped => command_receiver
                            .recv()
                            .map_err(|_| RecvTimeoutError::Disconnected),

                        AcquisitionState::Running { next_poll } => {
                            let timeout = next_poll.saturating_duration_since(now);

                            command_receiver.recv_timeout(timeout)
                        }
                    }
                };

                let poll_is_due = match &state {
                    AcquisitionState::Stopped => false,

                    AcquisitionState::Running { next_poll } => Instant::now() >= *next_poll,
                };

                let command_result = match command_result {
                    Ok(command) if poll_is_due => {
                        pending_command = Some(command);

                        continue;
                    }

                    result => result,
                };

                match command_result {
                    Ok(WorkerCommand::SetPollInterval(new_interval)) => {
                        if new_interval.is_zero() {
                            continue;
                        }

                        default_poll_interval = new_interval;

                        if let AcquisitionState::Running { next_poll } = &mut state {
                            *next_poll = Instant::now() + new_interval;
                        }
                    }

                    Ok(WorkerCommand::Start) => {
                        if matches!(state, AcquisitionState::Stopped) {
                            match source.start() {
                                Ok(()) => {
                                    series_schedules.clear();

                                    state = AcquisitionState::Running {
                                        next_poll: Instant::now() + default_poll_interval,
                                    };

                                    thread_running.store(true, Ordering::Release);

                                    let _ = event_sender.send(WorkerEvent::AcquisitionStarted);
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

                            series_schedules.clear();

                            thread_running.store(false, Ordering::Release);

                            let mut stopped_cleanly = true;

                            if let Err(error) = source.stop() {
                                stopped_cleanly = false;

                                let _ =
                                    event_sender.send(WorkerEvent::AcquisitionStopFailed(error));
                            }

                            if let Err(error) = sink.flush() {
                                stopped_cleanly = false;

                                let _ = event_sender.send(WorkerEvent::SampleSinkFailed(error));

                                sink = Box::new(NullSampleSink::new());

                                thread_sample_sink_active.store(false, Ordering::Release);
                            }

                            if stopped_cleanly {
                                let _ = event_sender.send(WorkerEvent::AcquisitionStopped);
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

                        let _ = event_sender.send(WorkerEvent::SeriesCleared);
                    }

                    Ok(WorkerCommand::StartCsvRecording(path)) => {
                        if let Err(error) = sink.flush() {
                            let _ = event_sender.send(WorkerEvent::SampleSinkFailed(error));
                        }

                        sink = Box::new(NullSampleSink::new());

                        thread_sample_sink_active.store(false, Ordering::Release);

                        match CsvSampleSink::create(&path) {
                            Ok(csv_sink) => {
                                sink = Box::new(csv_sink);

                                thread_sample_sink_active.store(true, Ordering::Release);

                                let _ = event_sender.send(WorkerEvent::RecordingStarted(path));
                            }

                            Err(error) => {
                                let _ = event_sender.send(WorkerEvent::SampleSinkFailed(error));
                            }
                        }
                    }

                    Ok(WorkerCommand::StopRecording) => {
                        let was_recording = thread_sample_sink_active.load(Ordering::Acquire);

                        let flush_result = sink.flush();

                        sink = Box::new(NullSampleSink::new());

                        thread_sample_sink_active.store(false, Ordering::Release);

                        match flush_result {
                            Ok(()) if was_recording => {
                                let _ = event_sender.send(WorkerEvent::RecordingStopped);
                            }

                            Ok(()) => {}

                            Err(error) => {
                                let _ = event_sender.send(WorkerEvent::SampleSinkFailed(error));
                            }
                        }
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

                    Ok(WorkerCommand::Connection(command)) => {
                        let acquisition_running =
                            matches!(&state, AcquisitionState::Running { .. });

                        handle_connection_command(
                            command,
                            acquisition_running,
                            source.as_mut(),
                            &event_sender,
                        );
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

    pub fn start(&self) -> Result<(), WorkerHandleError> {
        self.commands.start()
    }

    pub fn stop(&self) -> Result<(), WorkerHandleError> {
        self.commands.stop()
    }

    pub fn clear_series(&self) -> Result<(), WorkerHandleError> {
        self.commands.clear_series()
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub(crate) fn connection_id(&self) -> ConnectionId {
        self.commands.connection_id()
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

fn synchronize_series_schedules(
    schedules: &mut HashMap<SeriesId, SeriesSchedule>,
    series: &[SeriesMetadata],
    default_interval: Duration,
    now: Instant,
) {
    schedules.retain(|series_id, _| series.iter().any(|series| series.id == *series_id));

    for series in series {
        let interval = series
            .sampling_interval
            .map(|interval| interval.duration())
            .unwrap_or(default_interval);

        match schedules.get_mut(&series.id) {
            Some(schedule) if schedule.interval != interval => {
                schedule.interval = interval;
                schedule.next_poll = now + interval;
            }

            Some(_) => {}

            None => {
                schedules.insert(
                    series.id,
                    SeriesSchedule {
                        interval,
                        next_poll: now + interval,
                        consecutive_failures: 0,
                    },
                );
            }
        }
    }
}

fn next_series_poll(schedules: &HashMap<SeriesId, SeriesSchedule>) -> Option<Instant> {
    schedules.values().map(|schedule| schedule.next_poll).min()
}

fn due_series_metadata(
    series: &[SeriesMetadata],
    schedules: &HashMap<SeriesId, SeriesSchedule>,
    now: Instant,
) -> Vec<SeriesMetadata> {
    series
        .iter()
        .filter(|series| {
            schedules
                .get(&series.id)
                .is_some_and(|schedule| now >= schedule.next_poll)
        })
        .cloned()
        .collect()
}

fn advance_series_schedules(
    schedules: &mut HashMap<SeriesId, SeriesSchedule>,
    polled_series: &[SeriesMetadata],
    completed_at: Instant,
) {
    for series in polled_series {
        let Some(schedule) = schedules.get_mut(&series.id) else {
            continue;
        };

        schedule.next_poll += schedule.interval;

        if completed_at > schedule.next_poll + schedule.interval {
            schedule.next_poll = completed_at + schedule.interval;
        }
    }
}

fn update_series_health(
    schedules: &mut HashMap<SeriesId, SeriesSchedule>,
    polled_series: &[SeriesMetadata],
    failures: &[SeriesAcquisitionFailure],
    event_sender: &Sender<WorkerEvent>,
) {
    for series in polled_series {
        let Some(schedule) = schedules.get_mut(&series.id) else {
            continue;
        };

        let failure = failures
            .iter()
            .find(|failure| failure.series_id == series.id);

        match failure {
            Some(failure) => {
                schedule.consecutive_failures = schedule.consecutive_failures.saturating_add(1);

                let count = schedule.consecutive_failures;

                if should_report_series_failure(count) {
                    let _ = event_sender.send(WorkerEvent::SeriesPollingFailed {
                        id: series.id,
                        name: failure.series_name.clone(),
                        error: failure.error.clone(),
                        consecutive_failures: count,
                    });
                }
            }

            None => {
                let failed_attempts = std::mem::take(&mut schedule.consecutive_failures);

                if failed_attempts > 0 {
                    let _ = event_sender.send(WorkerEvent::SeriesPollingRecovered {
                        id: series.id,
                        name: series.name.clone(),
                        failed_attempts,
                    });
                }
            }
        }
    }
}

fn should_report_series_failure(failure_count: u64) -> bool {
    if failure_count == 0 {
        return false;
    }

    let mut value = failure_count;

    while value > 1 && value.is_multiple_of(10) {
        value /= 10;
    }

    value == 1
}

fn handle_connection_command(
    command: ConnectionCommand,
    acquisition_running: bool,
    source: &mut dyn AcquisitionSource,
    event_sender: &Sender<WorkerEvent>,
) {
    match command {
        ConnectionCommand::SendSerialText { config, command } => {
            let port_name = config.port_name().to_owned();

            let result = if acquisition_running {
                request_text_from_active_source(source, &command)
            } else {
                config
                    .open()
                    .and_then(|mut connection| connection.request_text(&command))
            };

            let event = match result {
                Ok(response) => WorkerEvent::SerialTextCommandSucceeded {
                    port_name,
                    command,
                    response,
                },

                Err(error) => WorkerEvent::SerialTextCommandFailed {
                    port_name,
                    command,
                    error,
                },
            };

            let _ = event_sender.send(event);
        }

        ConnectionCommand::ReadInstrument {
            port_name,
            request,
            response_sender,
        } => {
            let result = read_instrument_from_source(source, request);

            let result = close_source_after_one_shot(result, acquisition_running, source);

            let event = match &result {
                Ok(value) => WorkerEvent::InstrumentReadSucceeded {
                    port_name,
                    request,
                    value: *value,
                },

                Err(error) => WorkerEvent::InstrumentReadFailed {
                    port_name,
                    request,
                    error: error.clone(),
                },
            };

            let _ = event_sender.send(event);
            let _ = response_sender.send(result);
        }

        ConnectionCommand::WriteInstrument {
            port_name,
            request,
            response_sender,
        } => {
            let result = write_instrument_to_source(source, request);

            let result = close_source_after_one_shot(result, acquisition_running, source);

            let event = match &result {
                Ok(actual_value) => WorkerEvent::InstrumentWriteSucceeded {
                    port_name,
                    request,
                    actual_value: *actual_value,
                },

                Err(error) => WorkerEvent::InstrumentWriteFailed {
                    port_name,
                    request,
                    error: error.clone(),
                },
            };

            let _ = event_sender.send(event);
            let _ = response_sender.send(result);
        }

        ConnectionCommand::DescribeVirtualInstruments { response_sender } => {
            let result = describe_virtual_instruments_from_source(source);

            let result = close_source_after_one_shot(result, acquisition_running, source);

            let _ = response_sender.send(result);
        }
    }
}

fn close_source_after_one_shot<T>(
    mut result: Result<T, AcquisitionError>,
    acquisition_running: bool,
    source: &mut dyn AcquisitionSource,
) -> Result<T, AcquisitionError> {
    if acquisition_running {
        return result;
    }

    if let Err(stop_error) = source.stop() {
        result = match result {
            Ok(_) => Err(stop_error),

            Err(error) => Err(format!(
                "{error}; additionally failed to close \
                 the instrument source: {stop_error}",
            )
            .into()),
        };
    }

    result
}

fn append_series_samples(
    series: &mut [Series],
    samples: &[SeriesSample],
) -> Result<(), AcquisitionError> {
    for series_sample in samples {
        if !series
            .iter()
            .any(|series| series.id == series_sample.series_id)
        {
            return Err(format!(
                "Acquisition source returned a \
                 sample for unknown series {}",
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

fn read_instrument_from_source(
    source: &mut dyn AcquisitionSource,
    request: InstrumentReadRequest,
) -> InstrumentReadResult {
    source.read_instrument(request)?.ok_or_else(|| {
        AcquisitionError::from(
            "No acquisition source supports \
                 instrument reads",
        )
    })
}

fn request_text_from_active_source(
    source: &mut dyn AcquisitionSource,
    command: &str,
) -> Result<String, SerialConnectionError> {
    source
        .request_text(command)
        .map_err(|error| SerialConnectionError::from(error.to_string()))?
        .ok_or_else(|| {
            SerialConnectionError::from(
                "No acquisition source supports \
                 text COM commands",
            )
        })
}

fn write_instrument_to_source(
    source: &mut dyn AcquisitionSource,
    request: InstrumentWriteRequest,
) -> Result<crate::instrument::InstrumentValue, AcquisitionError> {
    source.write_instrument(request)?.ok_or_else(|| {
        AcquisitionError::from(
            "No acquisition source supports \
                 instrument writes",
        )
    })
}

fn describe_virtual_instruments_from_source(
    source: &mut dyn AcquisitionSource,
) -> VirtualInstrumentDescribeResult {
    source.describe_virtual_instruments()?.ok_or_else(|| {
        AcquisitionError::from(
            "No acquisition source supports \
                 virtual instrument discovery",
        )
    })
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        time::{Duration, Instant},
    };

    use super::{SeriesSchedule, should_report_series_failure, synchronize_series_schedules};

    use crate::connection::ConnectionId;
    use crate::data::{SamplingInterval, SeriesId, SeriesMetadata, SeriesSource};

    fn metadata(id: u64, sampling_interval: Option<SamplingInterval>) -> SeriesMetadata {
        SeriesMetadata {
            id: SeriesId::new(id),
            connection_id: ConnectionId::PRIMARY,
            name: format!("series_{id}"),

            source: SeriesSource::SerialCommand {
                command: "read".to_owned(),
            },

            sampling_interval,
            visible: true,
        }
    }

    #[test]
    fn uses_default_and_custom_intervals() {
        let now = Instant::now();

        let custom_interval = SamplingInterval::new(Duration::from_secs(5)).unwrap();

        let series = [metadata(1, None), metadata(2, Some(custom_interval))];

        let mut schedules = HashMap::new();

        synchronize_series_schedules(&mut schedules, &series, Duration::from_secs(1), now);

        assert_eq!(
            schedules[&SeriesId::new(1)].interval,
            Duration::from_secs(1),
        );

        assert_eq!(
            schedules[&SeriesId::new(1)].next_poll,
            now + Duration::from_secs(1),
        );

        assert_eq!(
            schedules[&SeriesId::new(2)].interval,
            Duration::from_secs(5),
        );

        assert_eq!(
            schedules[&SeriesId::new(2)].next_poll,
            now + Duration::from_secs(5),
        );
    }

    #[test]
    fn updates_only_default_intervals() {
        let first_now = Instant::now();

        let custom_interval = SamplingInterval::new(Duration::from_secs(5)).unwrap();

        let series = [metadata(1, None), metadata(2, Some(custom_interval))];

        let mut schedules: HashMap<SeriesId, SeriesSchedule> = HashMap::new();

        synchronize_series_schedules(&mut schedules, &series, Duration::from_secs(1), first_now);

        let custom_deadline = schedules[&SeriesId::new(2)].next_poll;

        let second_now = first_now + Duration::from_millis(100);

        synchronize_series_schedules(&mut schedules, &series, Duration::from_secs(2), second_now);

        assert_eq!(
            schedules[&SeriesId::new(1)].interval,
            Duration::from_secs(2),
        );

        assert_eq!(
            schedules[&SeriesId::new(1)].next_poll,
            second_now + Duration::from_secs(2),
        );

        assert_eq!(schedules[&SeriesId::new(2)].next_poll, custom_deadline,);
    }

    #[test]
    fn rate_limits_series_failure_reports() {
        for count in [1, 10, 100, 1_000] {
            assert!(should_report_series_failure(count),);
        }

        for count in [0, 2, 9, 11, 99, 101] {
            assert!(!should_report_series_failure(count),);
        }
    }
}
