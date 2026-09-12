//! Processing-thread event loop for filters, controllers and diagnostics.
//!
//! Sample processing and configuration commands are serialized in one owner. Control outputs leave
//! through a separate event channel for arbitration and hardware dispatch.

use std::hash::Hash;

use crossbeam_channel::{Receiver, Sender};

use super::{
    AddControllerDiagnosticError, ProcessingEvent, ProcessingInput, command::ProcessingCommand,
};
use crate::{
    process_control::{ControlEvent, ControllerDiagnostic, ControllerRegistry},
    signal_processing::{ProcessedSignal, SignalProcessingGraph},
};

#[derive(Clone, Debug, PartialEq)]
struct ControllerDiagnosticBinding<SignalId> {
    controller: String,
    diagnostic: ControllerDiagnostic,
    output: SignalId,
}

/// Binds a supported diagnostic to an unused signal ID. Diagnostics cannot collide with filter
/// outputs or other diagnostic streams.
fn add_controller_diagnostic<SignalId>(
    graph: &SignalProcessingGraph<SignalId>,
    registry: &ControllerRegistry<SignalId>,
    bindings: &mut Vec<ControllerDiagnosticBinding<SignalId>>,
    controller: String,
    diagnostic: ControllerDiagnostic,
    output: SignalId,
) -> Result<(), AddControllerDiagnosticError<SignalId>>
where
    SignalId: Copy + Eq + Hash,
{
    registry
        .validate_diagnostic(&controller, diagnostic)
        .map_err(AddControllerDiagnosticError::Controller)?;

    if graph.contains_output(output) || bindings.iter().any(|binding| binding.output == output) {
        return Err(AddControllerDiagnosticError::DuplicateOutput { output });
    }

    bindings.push(ControllerDiagnosticBinding {
        controller,
        diagnostic,
        output,
    });

    Ok(())
}

/// Finds loops consuming the removed signal or any downstream filter before graph mutation.
fn controllers_affected_by_removal<SignalId>(
    graph: &SignalProcessingGraph<SignalId>,
    registry: &ControllerRegistry<SignalId>,
    signal_id: SignalId,
) -> Vec<String>
where
    SignalId: Copy + Eq + Hash,
{
    let mut affected_signals = graph.removal_set_from(signal_id);

    if !affected_signals.contains(&signal_id) {
        affected_signals.insert(0, signal_id);
    }

    let mut controllers = Vec::new();

    for affected_signal in affected_signals {
        controllers.extend(registry.names_from(affected_signal));
    }

    controllers
}

/// Serializes graph and controller mutations with incoming samples on one owning thread.
/// Emits computed samples and control events through separate channels; performs no hardware I/O.
pub(super) fn run_processing<SignalId>(
    command_receiver: Receiver<ProcessingCommand<SignalId>>,
    event_sender: Sender<ProcessingEvent<SignalId>>,
    control_event_sender: Sender<ControlEvent<SignalId>>,
) where
    SignalId: Copy + Eq + Hash,
{
    let mut graph = SignalProcessingGraph::new();
    let mut registry = ControllerRegistry::new();
    let mut controller_diagnostics = Vec::new();

    while let Ok(command) = command_receiver.recv() {
        match command {
            ProcessingCommand::AddFilter {
                input,
                output,
                definition,
                response_sender,
            } => {
                let result = graph.add_filter(input, output, definition);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ReplaceFilter {
                output,
                definition,
                response_sender,
            } => {
                let affected_signals = graph.removal_set_from(output);

                let result = graph.replace_filter(output, definition);

                if result.is_ok() {
                    for signal_id in affected_signals {
                        registry.resynchronize_from(signal_id);
                    }
                }

                let _ = response_sender.send(result);
            }

            ProcessingCommand::AddControlLoop {
                definition,
                response_sender,
            } => {
                let result = registry.add(definition);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::AddControllerDiagnostic {
                controller,
                diagnostic,
                output,
                response_sender,
            } => {
                let result = add_controller_diagnostic(
                    &graph,
                    &registry,
                    &mut controller_diagnostics,
                    controller,
                    diagnostic,
                    output,
                );

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ControllerParameters {
                name,
                response_sender,
            } => {
                let result = registry.parameters(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ControllerDiagnostics {
                name,
                response_sender,
            } => {
                let result = registry.diagnostics(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ReadControllerParameter {
                name,
                key,
                response_sender,
            } => {
                let result = registry.read_parameter(&name, &key);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::WriteControllerParameter {
                name,
                key,
                value,
                response_sender,
            } => {
                let result = registry.write_parameter(&name, &key, value);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ConfigureController {
                name,
                updates,
                response_sender,
            } => {
                let result = registry.configure(&name, updates);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ReferenceKind {
                name,
                response_sender,
            } => {
                let result = registry.reference_kind(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ReferenceParameters {
                name,
                response_sender,
            } => {
                let result = registry.reference_parameters(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ReadReferenceParameter {
                name,
                key,
                response_sender,
            } => {
                let result = registry.read_reference_parameter(&name, &key);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::WriteReferenceParameter {
                name,
                key,
                value,
                response_sender,
            } => {
                let result = registry.write_reference_parameter(&name, &key, value);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ConfigureReference {
                name,
                updates,
                response_sender,
            } => {
                let result = registry.configure_reference(&name, updates);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::SetReference {
                name,
                source,
                response_sender,
            } => {
                let result = registry.set_reference(&name, source);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::SetControllerInput {
                name,
                input,
                response_sender,
            } => {
                let result = registry.set_input(&name, input);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ControllerState {
                name,
                response_sender,
            } => {
                let result = registry.state(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::RemoveController {
                name,
                response_sender,
            } => {
                let removed = registry.remove(&name);

                controller_diagnostics.retain(|binding| binding.controller != name);

                let _ = response_sender.send(removed);
            }

            ProcessingCommand::PauseController {
                name,
                response_sender,
            } => {
                let result = registry.pause(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ResumeController {
                name,
                response_sender,
            } => {
                let result = registry.resume(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ResetControllerIntegral {
                name,
                response_sender,
            } => {
                let result = registry.reset_integral(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::ResetController {
                name,
                response_sender,
            } => {
                let result = registry.reset(&name);

                let _ = response_sender.send(result);
            }

            ProcessingCommand::Process(inputs) => {
                process_inputs(
                    &mut graph,
                    &mut registry,
                    &controller_diagnostics,
                    inputs,
                    &event_sender,
                    &control_event_sender,
                );
            }

            ProcessingCommand::ResetFrom { signal_id } => {
                graph.reset_from(signal_id);

                registry.reset_from(signal_id);
            }

            ProcessingCommand::ControllerNames { response_sender } => {
                let _ = response_sender.send(registry.names());
            }

            ProcessingCommand::ControllersAffectedByRemoval {
                signal_id,
                response_sender,
            } => {
                let controllers = controllers_affected_by_removal(&graph, &registry, signal_id);

                let _ = response_sender.send(controllers);
            }

            ProcessingCommand::RemoveFrom {
                signal_id,
                response_sender,
            } => {
                let mut removed = graph.remove_from(signal_id);

                controller_diagnostics.retain(|binding| binding.output != signal_id);

                let mut removed_controllers = registry.remove_from(signal_id);

                let dependent_ids = removed.clone();

                for dependent_id in dependent_ids
                    .into_iter()
                    .filter(|removed_id| *removed_id != signal_id)
                {
                    removed_controllers.extend(registry.remove_from(dependent_id));
                }

                if !removed_controllers.is_empty() {
                    controller_diagnostics.retain(|binding| {
                        if removed_controllers
                            .iter()
                            .any(|controller| controller == &binding.controller)
                        {
                            removed.push(binding.output);

                            false
                        } else {
                            true
                        }
                    });
                }

                let _ = response_sender.send(removed);
            }

            ProcessingCommand::Clear { response_sender } => {
                controller_diagnostics.clear();
                registry.clear();
                graph.clear();

                let _ = response_sender.send(());
            }

            ProcessingCommand::Shutdown => {
                break;
            }
        }
    }
}

fn process_inputs<SignalId>(
    graph: &mut SignalProcessingGraph<SignalId>,
    registry: &mut ControllerRegistry<SignalId>,
    controller_diagnostics: &[ControllerDiagnosticBinding<SignalId>],
    inputs: Vec<ProcessingInput<SignalId>>,
    event_sender: &Sender<ProcessingEvent<SignalId>>,
    control_event_sender: &Sender<ControlEvent<SignalId>>,
) where
    SignalId: Copy + Eq + Hash,
{
    let mut output_samples = Vec::new();

    for input in inputs {
        process_control_events(
            registry.process(input.signal_id, input.timestamp, input.value),
            controller_diagnostics,
            &mut output_samples,
            control_event_sender,
        );

        match graph.process(input.signal_id, input.timestamp, input.value) {
            Ok(mut processed) => {
                for signal in &processed {
                    process_control_events(
                        registry.process(signal.signal_id, signal.timestamp, signal.value),
                        controller_diagnostics,
                        &mut output_samples,
                        control_event_sender,
                    );
                }

                output_samples.append(&mut processed);
            }

            Err(error) => {
                let _ = event_sender.send(ProcessingEvent::Error(error));
            }
        }
    }

    if !output_samples.is_empty() {
        let _ = event_sender.send(ProcessingEvent::Samples(output_samples));
    }
}

fn process_control_events<SignalId>(
    events: Vec<ControlEvent<SignalId>>,
    bindings: &[ControllerDiagnosticBinding<SignalId>],
    output_samples: &mut Vec<ProcessedSignal<SignalId>>,
    sender: &Sender<ControlEvent<SignalId>>,
) where
    SignalId: Copy + Eq,
{
    for event in events {
        if let ControlEvent::Output(control_output) = &event {
            for binding in bindings {
                if binding.controller != control_output.loop_name {
                    continue;
                }

                let Some(value) = control_output.output.diagnostic(binding.diagnostic) else {
                    continue;
                };

                output_samples.push(ProcessedSignal {
                    signal_id: binding.output,
                    timestamp: control_output.timestamp,
                    value,
                });
            }
        }

        let _ = sender.send(event);
    }
}
