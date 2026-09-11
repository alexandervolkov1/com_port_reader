use crate::{
    data::SeriesStore,
    process_recorder::ProcessAction,
    user_command::{
        AcquisitionCommand, ControllerCommand, EmulatorCommand, InstrumentCommand, SerialCommand,
        SeriesCommand, UserCommand,
    },
};

pub(super) fn resolve_action_series_id(action: &mut ProcessAction, series: &SeriesStore) {
    let name = match action {
        ProcessAction::SetFilter { name, .. }
        | ProcessAction::DeleteSeriesByName { name, .. }
        | ProcessAction::SetSeriesColor { name, .. } => Some(name.as_str()),

        ProcessAction::RenameSeries { current_name, .. } => Some(current_name.as_str()),

        _ => None,
    };

    let series_id = name.and_then(|name| series.id_by_name(name));

    match action {
        ProcessAction::SetFilter {
            series_id: stored_id,
            ..
        }
        | ProcessAction::DeleteSeriesByName {
            series_id: stored_id,
            ..
        }
        | ProcessAction::RenameSeries {
            series_id: stored_id,
            ..
        }
        | ProcessAction::SetSeriesColor {
            series_id: stored_id,
            ..
        } => {
            *stored_id = series_id;
        }

        _ => {}
    }
}

pub(super) fn process_action_from_command(command: &UserCommand) -> Option<ProcessAction> {
    match command {
        UserCommand::Scenario(_) => None,

        UserCommand::Acquisition(command) => Some(match command {
            AcquisitionCommand::Start => ProcessAction::StartAcquisition,

            AcquisitionCommand::Stop => ProcessAction::StopAcquisition,
        }),

        UserCommand::Emulator(command) => Some(match command {
            EmulatorCommand::Start => ProcessAction::StartEmulator,

            EmulatorCommand::Stop => ProcessAction::StopEmulator,
        }),

        UserCommand::Instrument(command) => Some(match command {
            InstrumentCommand::Read {
                connection_id,
                request,
                ..
            } => ProcessAction::ReadInstrument {
                connection_id: *connection_id,
                request: request.to_string(),
            },

            InstrumentCommand::Write {
                connection_id,
                request,
                ..
            } => ProcessAction::WriteInstrument {
                connection_id: *connection_id,
                request: request.to_string(),
            },

            InstrumentCommand::DescribeVirtualInstruments { connection_id, .. } => {
                ProcessAction::DescribeVirtualInstruments {
                    connection_id: *connection_id,
                }
            }
        }),

        UserCommand::Serial(command) => Some(match command {
            SerialCommand::SendText {
                connection_id,
                command,
            } => ProcessAction::SendSerial {
                connection_id: *connection_id,
                command: command.clone(),
            },
        }),

        UserCommand::Series(command) => match command {
            SeriesCommand::Add(new_series) => Some(ProcessAction::AddSeries {
                connection_id: new_series.connection_id(),

                name: new_series.name().map(str::to_owned),

                source: new_series.source().to_string(),

                polling_interval_seconds: new_series
                    .sampling_interval()
                    .map(|interval| interval.duration().as_secs_f64()),

                color: new_series.color().map(|color| color.to_string()),
            }),

            SeriesCommand::AddFilter(filter) => Some(ProcessAction::AddFilteredSeries {
                input_name: filter.input_name().to_owned(),

                name: filter.name().to_owned(),

                definition: filter.definition().to_string(),

                color: filter.color().map(|color| color.to_string()),
            }),

            SeriesCommand::SetFilter { name, definition } => Some(ProcessAction::SetFilter {
                series_id: None,
                name: name.clone(),
                definition: definition.to_string(),
            }),

            SeriesCommand::Delete { name } => Some(ProcessAction::DeleteSeriesByName {
                series_id: None,
                name: name.clone(),
            }),

            SeriesCommand::Rename {
                current_name,
                new_name,
            } => Some(ProcessAction::RenameSeries {
                series_id: None,
                current_name: current_name.clone(),
                new_name: new_name.clone(),
            }),

            SeriesCommand::SetColor { name, color } => Some(ProcessAction::SetSeriesColor {
                series_id: None,
                name: name.clone(),
                color: color.map(|color| color.to_string()),
            }),

            SeriesCommand::Clear => Some(ProcessAction::ClearSeries),

            SeriesCommand::SetPane { .. }
            | SeriesCommand::Retry { .. }
            | SeriesCommand::RetryAll => None,
        },

        UserCommand::Controller(command) => match command {
            ControllerCommand::Add(new_controller) => {
                let controller = new_controller.controller();

                let parameters = controller.parameter_values().ok()?;

                Some(ProcessAction::AddController {
                    connection_id: new_controller.output_target().connection_id(),

                    name: new_controller.name().to_owned(),

                    input_name: new_controller.input_name().to_owned(),

                    output_target: new_controller.output_target().to_string(),

                    kind: controller.kind(),

                    parameters,
                })
            }

            ControllerCommand::WriteParameter {
                name, key, value, ..
            } => Some(ProcessAction::WriteControllerParameter {
                name: name.clone(),
                key: key.clone(),
                value: *value,
            }),

            ControllerCommand::Configure { name, updates, .. } => {
                Some(ProcessAction::ConfigureController {
                    name: name.clone(),
                    updates: updates.clone(),
                })
            }

            ControllerCommand::WriteReferenceParameter {
                name, key, value, ..
            } => Some(ProcessAction::WriteControllerReferenceParameter {
                name: name.clone(),
                key: key.clone(),
                value: *value,
            }),

            ControllerCommand::ConfigureReference { name, updates, .. } => {
                Some(ProcessAction::ConfigureControllerReference {
                    name: name.clone(),
                    updates: updates.clone(),
                })
            }

            ControllerCommand::SetReference { name, source, .. } => {
                Some(ProcessAction::SetControllerReference {
                    name: name.clone(),
                    source: *source,
                })
            }

            ControllerCommand::SetInput {
                name, input_name, ..
            } => Some(ProcessAction::SetControllerInput {
                name: name.clone(),
                input_name: input_name.clone(),
            }),

            ControllerCommand::Remove { name, .. } => {
                Some(ProcessAction::RemoveController { name: name.clone() })
            }

            ControllerCommand::Pause { name, .. } => {
                Some(ProcessAction::PauseController { name: name.clone() })
            }

            ControllerCommand::Resume { name, .. } => {
                Some(ProcessAction::ResumeController { name: name.clone() })
            }

            ControllerCommand::ResetIntegral { name, .. } => {
                Some(ProcessAction::ResetControllerIntegral { name: name.clone() })
            }

            ControllerCommand::Reset { name, .. } => {
                Some(ProcessAction::ResetController { name: name.clone() })
            }

            ControllerCommand::AddDiagnostic(_)
            | ControllerCommand::Parameters { .. }
            | ControllerCommand::Diagnostics { .. }
            | ControllerCommand::ReadParameter { .. }
            | ControllerCommand::ReferenceKind { .. }
            | ControllerCommand::ReferenceParameters { .. }
            | ControllerCommand::ReadReferenceParameter { .. }
            | ControllerCommand::State { .. } => None,
        },

        UserCommand::Log { .. } => None,
    }
}
