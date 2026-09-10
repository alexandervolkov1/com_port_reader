use std::collections::BTreeSet;

use crate::{
    acquisition::AcquisitionError,
    app_log::LogHandle,
    connection::ConnectionId,
    data::{NewFilteredSeries, NewSeries, SeriesId, SeriesSource, SeriesStore},
    output_control::OutputHandle,
    process_recorder::{ProcessActionContext, ProcessRecorder},
    signal_processing::{ProcessingHandle, SignalFilterDefinition},
    user_command::{PauseControllerError, SeriesCommand},
    worker::{ConnectionRouter, WorkerHandle},
};

use super::controller_command_handler::pause_controller_safely;

pub(crate) struct SeriesCommandHandler<'a> {
    connections: &'a ConnectionRouter,
    series: &'a SeriesStore,
    processing: &'a ProcessingHandle<SeriesId>,
    output_control: &'a OutputHandle,
    process_recorder: &'a ProcessRecorder,
    log: &'a LogHandle,
}

impl<'a> SeriesCommandHandler<'a> {
    pub(crate) fn new(
        connections: &'a ConnectionRouter,
        series: &'a SeriesStore,
        processing: &'a ProcessingHandle<SeriesId>,
        output_control: &'a OutputHandle,
        process_recorder: &'a ProcessRecorder,
        log: &'a LogHandle,
    ) -> Self {
        Self {
            connections,
            series,
            processing,
            output_control,
            process_recorder,
            log,
        }
    }

    pub(crate) fn execute(
        &self,
        command: SeriesCommand,
        action_context: Option<ProcessActionContext>,
    ) {
        match command {
            SeriesCommand::Add(new_series) => match self.add_series(new_series) {
                Ok(id) => {
                    self.record_applied(action_context, Some(id), self.series_name(id));
                }

                Err(error) => {
                    self.record_failed(action_context, error);
                }
            },

            SeriesCommand::AddFilter(filter) => match self.add_filter(filter) {
                Ok(id) => {
                    self.record_applied(action_context, Some(id), self.series_name(id));
                }

                Err(error) => {
                    self.record_failed(action_context, error);
                }
            },

            SeriesCommand::SetFilter { name, definition } => {
                match self.set_filter(&name, definition) {
                    Ok(id) => {
                        self.record_applied(action_context, Some(id), Some(name));
                    }

                    Err(error) => {
                        self.record_failed(action_context, error);
                    }
                }
            }

            SeriesCommand::Delete { name } => match self.delete_series(&name) {
                Ok(id) => {
                    self.record_applied(action_context, Some(id), Some(name));
                }

                Err(error) => {
                    self.record_failed(action_context, error);
                }
            },

            SeriesCommand::Rename {
                current_name,
                new_name,
            } => match self.series.rename_series(&current_name, &new_name) {
                Ok(id) => {
                    self.record_applied(action_context, Some(id), Some(new_name));
                }

                Err(error) => {
                    self.record_failed(action_context, error.to_string());
                }
            },

            SeriesCommand::SetColor { name, color } => {
                match self.series.set_color_by_name(&name, color) {
                    Some(id) => {
                        self.record_applied(action_context, Some(id), Some(name));
                    }

                    None => {
                        self.record_failed(action_context, format!("Series '{name}' not found.",));
                    }
                }
            }

            SeriesCommand::Retry { name } => {
                self.retry_series(name);
            }

            SeriesCommand::RetryAll => {
                self.retry_all_series();
            }

            SeriesCommand::Clear => match self.clear_series() {
                Ok(()) => {
                    self.record_applied(action_context, None, None);
                }

                Err(error) => {
                    self.record_failed(action_context, error);
                }
            },
        }
    }

    fn record_applied(
        &self,
        action_context: Option<ProcessActionContext>,
        series_id: Option<SeriesId>,
        series_name: Option<String>,
    ) {
        if let Some(action_context) = action_context {
            self.process_recorder.record_action_applied(
                action_context.action_id(),
                series_id,
                series_name,
            );
        }
    }

    fn record_failed(
        &self,
        action_context: Option<ProcessActionContext>,
        error: impl Into<String>,
    ) {
        if let Some(action_context) = action_context {
            self.process_recorder
                .record_action_failed(action_context.action_id(), error.into());
        }
    }

    fn series_name(&self, id: SeriesId) -> Option<String> {
        self.series
            .metadata()
            .into_iter()
            .find(|series| series.id == id)
            .map(|series| series.name)
    }

    fn add_series(&self, new_series: NewSeries) -> Result<SeriesId, String> {
        self.series
            .add_series(new_series)
            .map_err(|error| format!("Failed to add series: {error}",))
    }

    fn add_filter(&self, filter: NewFilteredSeries) -> Result<SeriesId, String> {
        let (input_name, output_name, definition, color) = filter.into_parts();

        let Some(input_id) = self.series.id_by_name(&input_name) else {
            return Err(format!(
                "Signal processing failed: \
                 cannot add filtered series \
                 '{output_name}': input series \
                 '{input_name}' was not found",
            ));
        };

        let mut new_series = NewSeries::named_filtered(input_id, definition, output_name.clone());

        if let Some(color) = color {
            new_series = new_series.with_color(color);
        }

        let output_id = self.series.add_series(new_series).map_err(|error| {
            format!(
                "Failed to add series: \
                     {error}",
            )
        })?;

        if let Err(error) = self.processing.add_filter(input_id, output_id, definition) {
            self.series.remove_series(output_id);

            return Err(format!(
                "Signal processing failed: \
                 cannot add filtered series \
                 '{output_name}' from \
                 '{input_name}': {error}",
            ));
        }

        Ok(output_id)
    }

    fn set_filter(
        &self,
        name: &str,
        definition: SignalFilterDefinition,
    ) -> Result<SeriesId, String> {
        let Some(output_id) = self.series.id_by_name(name) else {
            return Err(format!("Series '{name}' not found.",));
        };

        let old_definition = self
            .series
            .metadata()
            .into_iter()
            .find(|series| series.id == output_id)
            .and_then(|series| match series.source {
                SeriesSource::Filtered { definition, .. } => Some(definition),

                _ => None,
            })
            .ok_or_else(|| {
                format!(
                    "Signal processing failed: \
                     cannot change filter for \
                     series '{name}': series is \
                     not a filtered series",
                )
            })?;

        self.processing
            .replace_filter(output_id, definition)
            .map_err(|error| {
                format!(
                    "Signal processing failed: \
                     cannot change filter for \
                     series '{name}': {error}",
                )
            })?;

        if !self.series.set_filter_definition(output_id, definition) {
            let rollback_result = self.processing.replace_filter(output_id, old_definition);

            return match rollback_result {
                Ok(()) => Err(format!(
                    "Signal processing failed: \
                     cannot update stored filter \
                     definition for series \
                     '{name}'",
                )),

                Err(rollback_error) => Err(format!(
                    "Signal processing failed: \
                     cannot update stored filter \
                     definition for series \
                     '{name}'; processing rollback \
                     also failed: {rollback_error}",
                )),
            };
        }

        Ok(output_id)
    }

    fn retry_series(&self, name: String) {
        let Some((id, connection_id, was_suspended)) = self.series.resume_polling_by_name(&name)
        else {
            self.log.error(format!("Series '{name}' not found.",));

            return;
        };

        if !was_suspended {
            self.log.info(format!(
                "Series '{name}' ({id}) polling \
                 is already enabled.",
            ));

            return;
        }

        let worker = match self.connection_worker(connection_id) {
            Ok(worker) => worker,

            Err(error) => {
                self.log.error(error.to_string());

                return;
            }
        };

        if let Err(error) = worker.refresh_series_schedule() {
            self.log.error(format!("Failed to send command: {error}",));

            return;
        }

        self.log.info(format!(
            "Series '{name}' ({id}) polling \
             retry requested.",
        ));
    }

    fn retry_all_series(&self) {
        let resumed = self.series.resume_all_polling();

        if resumed.is_empty() {
            self.log.info(
                "There are no suspended series \
                 to retry.",
            );

            return;
        }

        let connection_ids = resumed
            .iter()
            .map(|(_, _, connection_id)| *connection_id)
            .collect::<BTreeSet<_>>();

        for connection_id in connection_ids {
            let worker = match self.connection_worker(connection_id) {
                Ok(worker) => worker,

                Err(error) => {
                    self.log.error(error.to_string());

                    continue;
                }
            };

            if let Err(error) = worker.refresh_series_schedule() {
                self.log.error(format!(
                    "Failed to send command: \
                     {error}",
                ));
            }
        }

        self.log.info(format!(
            "Polling retry requested for {} \
             suspended series.",
            resumed.len(),
        ));
    }

    fn connection_worker(
        &self,
        connection_id: ConnectionId,
    ) -> Result<WorkerHandle, AcquisitionError> {
        self.connections.handle(connection_id).ok_or_else(|| {
            AcquisitionError::from(format!(
                "Connection worker \
                     {connection_id:?} is not \
                     registered",
            ))
        })
    }

    fn pause_controller(&self, name: &str) -> Result<(), PauseControllerError> {
        pause_controller_safely(self.output_control, self.processing, name)
    }

    fn delete_series(&self, name: &str) -> Result<SeriesId, String> {
        let Some(id) = self.series.id_by_name(name) else {
            return Err(format!("Series '{name}' not found.",));
        };

        let affected_controllers = self
            .processing
            .controllers_affected_by_removal(id)
            .map_err(|error| {
                format!(
                    "Processing failed: \
                     cannot preview controllers \
                     affected by removal of \
                     series '{name}': {error}",
                )
            })?;

        for controller in &affected_controllers {
            self.pause_controller(controller).map_err(|error| {
                format!(
                    "Cannot remove series \
                         '{name}': failed to safely \
                         pause controller \
                         '{controller}': {error}",
                )
            })?;
        }

        let dependent_ids = self.processing.remove_from(id).map_err(|error| {
            format!(
                "Processing failed: \
                     cannot remove processing \
                     branch for series '{name}': \
                     {error}",
            )
        })?;

        for controller in &affected_controllers {
            if let Err(error) = self.output_control.release_controller(controller) {
                self.log.error(format!(
                    "Controller '{controller}' \
                     was removed from processing, \
                     but its output ownership \
                     could not be released: \
                     {error}",
                ));
            }
        }

        for dependent_id in dependent_ids
            .iter()
            .copied()
            .filter(|dependent_id| *dependent_id != id)
        {
            self.series.remove_series(dependent_id);
        }

        self.series.remove_series(id);

        Ok(id)
    }

    fn clear_series(&self) -> Result<(), String> {
        let controllers = self.processing.controller_names().map_err(|error| {
            format!(
                "Processing failed: \
                     cannot inspect controllers \
                     before clearing: {error}",
            )
        })?;

        for controller in &controllers {
            self.pause_controller(controller).map_err(|error| {
                format!(
                    "Cannot clear processing: \
                         failed to safely pause \
                         controller \
                         '{controller}': {error}",
                )
            })?;
        }

        self.processing.clear().map_err(|error| {
            format!(
                "Processing failed: \
                     cannot clear processing \
                     state: {error}",
            )
        })?;

        for controller in &controllers {
            if let Err(error) = self.output_control.release_controller(controller) {
                self.log.error(format!(
                    "Controller '{controller}' \
                     was removed from processing, \
                     but its output ownership \
                     could not be released: \
                     {error}",
                ));
            }
        }

        self.series.clear();

        Ok(())
    }
}
