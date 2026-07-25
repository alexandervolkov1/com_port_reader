use chrono::Local;
use crossbeam_channel::{Receiver, Sender};
use std::path::{Path, PathBuf};

use crate::{
    acquisition::AcquisitionSource,
    app_log::LogHandle,
    data::SeriesStore,
    sample_sink::SampleSink,
    worker::{Worker, WorkerCommand, WorkerConfig, WorkerEvent, WorkerHandle, WorkerHandleError},
};

pub struct ControlsModel {
    worker: Worker,
    recording_file: Option<PathBuf>,
    recording_error: Option<String>,
    log: LogHandle,
}

impl ControlsModel {
    pub fn new(
        series: SeriesStore,
        worker_handle: WorkerHandle,
        command_receiver: Receiver<WorkerCommand>,
        event_sender: Sender<WorkerEvent>,
        source: Box<dyn AcquisitionSource>,
        sink: Box<dyn SampleSink>,
        config: WorkerConfig,
        log: LogHandle,
    ) -> Self {
        let worker = Worker::spawn(
            worker_handle,
            command_receiver,
            event_sender,
            series,
            source,
            sink,
            config,
        );

        Self {
            worker,
            recording_file: None,
            recording_error: None,
            log,
        }
    }

    pub fn start(&self) {
        self.report_worker_error("start acquisition", self.worker.start());
    }

    pub fn stop(&self) {
        self.report_worker_error("stop acquisition", self.worker.stop());
    }

    pub fn clear(&self) {
        self.report_worker_error("clear series", self.worker.clear_series());
    }

    pub fn is_running(&self) -> bool {
        self.worker.is_running()
    }

    pub fn start_recording(&mut self) {
        let now = Local::now();

        let date = now.format("%Y-%m-%d").to_string();

        let file_name = now.format("protocol %Y-%m-%d %H-%M-%S.csv").to_string();

        let path = PathBuf::from("protocols").join(date).join(file_name);

        match self.worker.start_csv_recording(path.clone()) {
            Ok(()) => {
                self.recording_file = Some(path);
                self.recording_error = None;
            }

            Err(error) => {
                let message = format!("Failed to start recording: {error}");

                self.recording_error = Some(message.clone());
                self.log.error(message);
            }
        }
    }

    pub fn stop_recording(&mut self) {
        match self.worker.stop_recording() {
            Ok(()) => {
                self.recording_error = None;
            }

            Err(error) => {
                let message = format!("Failed to stop recording: {error}");

                self.recording_error = Some(message.clone());
                self.log.error(message);
            }
        }
    }

    pub fn is_recording(&self) -> bool {
        self.worker.is_recording()
    }

    pub fn recording_file(&self) -> Option<&Path> {
        self.recording_file.as_deref()
    }

    pub fn recording_error(&self) -> Option<&str> {
        self.recording_error.as_deref()
    }

    fn report_worker_error(&self, action: &str, result: Result<(), WorkerHandleError>) {
        if let Err(error) = result {
            self.log.error(format!("Failed to {action}: {error}"));
        }
    }
}
