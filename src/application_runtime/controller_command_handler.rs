use crate::{
    app_log::LogHandle,
    data::{NewControllerDiagnosticSeries, NewSeries, SeriesId, SeriesStore},
    instrument::ConnectedParameterAddress,
    output_control::OutputHandle,
    process_control::{ControlLoopDefinition, ControlOutputTarget, NewController},
    process_recorder::{ProcessActionContext, ProcessRecorder},
    signal_processing::ProcessingHandle,
    user_command::{
        ControllerCommand, PauseControllerError, ResumeControllerError, SetControllerInputError,
    },
};

pub(super) fn pause_controller_safely(
    output_control: &OutputHandle,
    processing: &ProcessingHandle<SeriesId>,
    name: &str,
) -> Result<(), PauseControllerError> {
    let safe_output_result = output_control.apply_safe(name);

    let pause_result = processing.pause_controller(name);

    match safe_output_result {
        Err(output) => match pause_result {
            Ok(()) => Err(PauseControllerError::Output(output)),

            Err(pause) => Err(PauseControllerError::OutputAndControllerPause { output, pause }),
        },

        Ok(response) => {
            let safe_write_result = response.recv();

            match (safe_write_result, pause_result) {
                (Ok(_), Ok(())) => Ok(()),

                (Err(write), Ok(())) => Err(PauseControllerError::SafeOutputWrite(write)),

                (Ok(_), Err(pause)) => Err(PauseControllerError::ControllerAfterSafeOutput(pause)),

                (Err(write), Err(pause)) => {
                    Err(PauseControllerError::SafeOutputWriteAndControllerPause { write, pause })
                }
            }
        }
    }
}

pub(super) fn resume_controller_safely(
    output_control: &OutputHandle,
    processing: &ProcessingHandle<SeriesId>,
    name: &str,
) -> Result<(), ResumeControllerError> {
    output_control.request_automatic(name)?;

    if let Err(controller_error) = processing.resume_controller(name) {
        if let Err(rollback_error) = output_control.rollback_automatic_request(name) {
            return Err(ResumeControllerError::Rollback {
                controller: controller_error,
                rollback: rollback_error,
            });
        }

        return Err(ResumeControllerError::Controller(controller_error));
    }

    Ok(())
}

pub(crate) struct ControllerCommandHandler<'a> {
    series: &'a SeriesStore,
    processing: &'a ProcessingHandle<SeriesId>,
    output_control: &'a OutputHandle,
    process_recorder: &'a ProcessRecorder,
    log: &'a LogHandle,
}

impl<'a> ControllerCommandHandler<'a> {
    pub(crate) fn new(
        series: &'a SeriesStore,
        processing: &'a ProcessingHandle<SeriesId>,
        output_control: &'a OutputHandle,
        process_recorder: &'a ProcessRecorder,
        log: &'a LogHandle,
    ) -> Self {
        Self {
            series,
            processing,
            output_control,
            process_recorder,
            log,
        }
    }

    pub(crate) fn execute(
        &self,
        command: ControllerCommand,
        action_context: Option<ProcessActionContext>,
    ) {
        match command {
            ControllerCommand::AddDiagnostic(diagnostic) => {
                self.add_controller_diagnostic(diagnostic);
            }

            ControllerCommand::Add(new_controller) => {
                let result = self.add_controller(new_controller);

                self.record_result(action_context, &result);
            }

            ControllerCommand::Parameters {
                name,
                response_sender,
            } => {
                let result = self.processing.controller_parameters(&name);

                let _ = response_sender.send(result);
            }

            ControllerCommand::Diagnostics {
                name,
                response_sender,
            } => {
                let result = self.processing.controller_diagnostics(&name);

                let _ = response_sender.send(result);
            }

            ControllerCommand::ReadParameter {
                name,
                key,
                response_sender,
            } => {
                let result = self.processing.read_controller_parameter(&name, &key);

                let _ = response_sender.send(result);
            }

            ControllerCommand::WriteParameter {
                name,
                key,
                value,
                response_sender,
            } => {
                let result = self
                    .processing
                    .write_controller_parameter(&name, &key, value);

                self.record_result(action_context, &result);

                let _ = response_sender.send(result);
            }

            ControllerCommand::Configure {
                name,
                updates,
                response_sender,
            } => {
                let result = self.processing.configure_controller(&name, updates);

                self.record_result(action_context, &result);

                let _ = response_sender.send(result);
            }

            ControllerCommand::ReferenceKind {
                name,
                response_sender,
            } => {
                let result = self.processing.reference_kind(&name);

                let _ = response_sender.send(result);
            }

            ControllerCommand::ReferenceParameters {
                name,
                response_sender,
            } => {
                let result = self.processing.reference_parameters(&name);

                let _ = response_sender.send(result);
            }

            ControllerCommand::ReadReferenceParameter {
                name,
                key,
                response_sender,
            } => {
                let result = self.processing.read_reference_parameter(&name, &key);

                let _ = response_sender.send(result);
            }

            ControllerCommand::WriteReferenceParameter {
                name,
                key,
                value,
                response_sender,
            } => {
                let result = self
                    .processing
                    .write_reference_parameter(&name, &key, value);

                self.record_result(action_context, &result);

                let _ = response_sender.send(result);
            }

            ControllerCommand::ConfigureReference {
                name,
                updates,
                response_sender,
            } => {
                let result = self.processing.configure_reference(&name, updates);

                self.record_result(action_context, &result);

                let _ = response_sender.send(result);
            }

            ControllerCommand::SetReference {
                name,
                source,
                response_sender,
            } => {
                let result = self.processing.set_reference(&name, source);

                self.record_result(action_context, &result);

                let _ = response_sender.send(result);
            }

            ControllerCommand::SetInput {
                name,
                input_name,
                response_sender,
            } => {
                let result = match self.series.id_by_name(&input_name) {
                    Some(input_id) => self
                        .processing
                        .set_controller_input(&name, input_id)
                        .map_err(Into::into),

                    None => Err(SetControllerInputError::SeriesNotFound(input_name)),
                };

                self.record_result(action_context, &result);

                let _ = response_sender.send(result);
            }

            ControllerCommand::State {
                name,
                response_sender,
            } => {
                let result = self.processing.controller_state(&name);

                let _ = response_sender.send(result);
            }

            ControllerCommand::Pause {
                name,
                response_sender,
            } => {
                let result = pause_controller_safely(self.output_control, self.processing, &name);

                self.record_result(action_context, &result);

                let _ = response_sender.send(result);
            }

            ControllerCommand::Resume {
                name,
                response_sender,
            } => {
                let result = resume_controller_safely(self.output_control, self.processing, &name);

                self.record_result(action_context, &result);

                let _ = response_sender.send(result);
            }

            ControllerCommand::ResetIntegral {
                name,
                response_sender,
            } => {
                let result = self.processing.reset_controller_integral(&name);

                self.record_result(action_context, &result);

                let _ = response_sender.send(result);
            }

            ControllerCommand::Reset {
                name,
                response_sender,
            } => {
                let result = self.processing.reset_controller(&name);

                self.record_result(action_context, &result);

                let _ = response_sender.send(result);
            }
        }
    }

    fn record_result<T, E>(
        &self,
        action_context: Option<ProcessActionContext>,
        result: &Result<T, E>,
    ) where
        E: ToString,
    {
        let Some(action_context) = action_context else {
            return;
        };

        match result {
            Ok(_) => {
                self.process_recorder
                    .record_action_applied(action_context.action_id(), None, None);
            }

            Err(error) => {
                self.process_recorder
                    .record_action_failed(action_context.action_id(), error.to_string());
            }
        }
    }

    fn install_control_loop(
        &self,
        name: &str,
        target: ConnectedParameterAddress,
        definition: ControlLoopDefinition<SeriesId, ControlOutputTarget>,
    ) -> Result<(), String> {
        let instance_id = definition.instance_id();

        let safe_request = definition
            .output_target()
            .safe_write_request()
            .map_err(|error| {
                format!(
                    "failed to create safe output \
                     request: {error}",
                )
            })?;

        self.output_control
            .register_controller(target, name, instance_id, safe_request)
            .map_err(|error| {
                format!(
                    "failed to register output: \
                     {error}",
                )
            })?;

        if let Err(error) = self.processing.add_control_loop(definition) {
            let rollback_result = self
                .output_control
                .rollback_controller_registration(target, name);

            return match rollback_result {
                Ok(()) => Err(error.to_string()),

                Err(rollback_error) => Err(format!(
                    "{error}; output ownership \
                         rollback also failed: \
                         {rollback_error}",
                )),
            };
        }

        Ok(())
    }

    fn add_controller(
        &self,
        new_controller: NewController<ControlOutputTarget>,
    ) -> Result<(), String> {
        let (name, input_name, output_target, controller) = new_controller.into_parts();

        let kind = controller.kind();

        let target = output_target.connected_parameter_address();

        let Some(input_id) = self.series.id_by_name(&input_name) else {
            return Err(format!(
                "Failed to add {kind} controller \
                 '{name}': input series \
                 '{input_name}' was not found",
            ));
        };

        let definition =
            ControlLoopDefinition::new(name.clone(), input_id, output_target, controller).map_err(
                |error| {
                    format!(
                        "Failed to add {kind} \
                     controller '{name}': \
                     {error}",
                    )
                },
            )?;

        self.install_control_loop(&name, target, definition)
            .map_err(|error| {
                format!(
                    "Failed to add {kind} controller \
                 '{name}': {error}",
                )
            })?;

        Ok(())
    }

    fn add_controller_diagnostic(&self, diagnostic_series: NewControllerDiagnosticSeries) {
        let (controller, diagnostic, name, connection_id, color) = diagnostic_series.into_parts();

        let mut new_series =
            NewSeries::named_controller_diagnostic(controller.clone(), diagnostic, name.clone())
                .with_connection(connection_id);

        if let Some(color) = color {
            new_series = new_series.with_color(color);
        }

        let output_id = match self.series.add_series(new_series) {
            Ok(output_id) => output_id,

            Err(error) => {
                self.log.error(format!(
                    "Failed to add controller \
                         diagnostic series \
                         '{name}': {error}",
                ));

                return;
            }
        };

        if let Err(error) =
            self.processing
                .add_controller_diagnostic(controller.clone(), diagnostic, output_id)
        {
            self.series.remove_series(output_id);

            self.log.error(format!(
                "Signal processing failed: \
                 cannot add diagnostic series \
                 '{name}' for controller \
                 '{controller}': {error}",
            ));

            return;
        }

        self.log.info(format!(
            "Controller diagnostic series \
             '{name}' ({output_id}) added for \
             controller '{controller}' \
             diagnostic '{diagnostic}'.",
        ));
    }
}
