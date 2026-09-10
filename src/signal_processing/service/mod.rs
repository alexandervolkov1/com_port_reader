mod command;
mod error;
mod handle;
mod runtime;

use std::{
    hash::Hash,
    io,
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, unbounded};
pub use error::{
    AddControlLoopError, AddControllerDiagnosticError, AddSignalFilterError,
    ControllerRequestError, ProcessingServiceDisconnected, ReplaceSignalFilterError,
};
pub use handle::ProcessingHandle;

use self::{command::ProcessingCommand, runtime::run_processing};
#[cfg(test)]
use crate::process_control::ControlLoopState;
use crate::{
    process_control::ControlEvent,
    signal_processing::{ProcessedSignal, SignalProcessingError},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProcessingInput<SignalId> {
    pub signal_id: SignalId,
    pub timestamp: f64,
    pub value: f64,
}

impl<SignalId> ProcessingInput<SignalId> {
    pub const fn new(signal_id: SignalId, timestamp: f64, value: f64) -> Self {
        Self {
            signal_id,
            timestamp,
            value,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProcessingEvent<SignalId> {
    Samples(Vec<ProcessedSignal<SignalId>>),
    Error(SignalProcessingError<SignalId>),
}

pub struct ProcessingService<SignalId> {
    handle: ProcessingHandle<SignalId>,
    event_receiver: Receiver<ProcessingEvent<SignalId>>,
    control_event_receiver: Receiver<ControlEvent<SignalId>>,
    thread: Option<JoinHandle<()>>,
}

impl<SignalId> ProcessingService<SignalId>
where
    SignalId: Copy + Eq + Hash + Send + 'static,
{
    pub fn spawn() -> io::Result<Self> {
        let (command_sender, command_receiver) = unbounded();

        let (event_sender, event_receiver) = unbounded();

        let (control_event_sender, control_event_receiver) = unbounded();

        let thread = thread::Builder::new()
            .name("processing".to_owned())
            .spawn(move || {
                run_processing(command_receiver, event_sender, control_event_sender);
            })?;

        Ok(Self {
            handle: ProcessingHandle { command_sender },

            event_receiver,

            control_event_receiver,

            thread: Some(thread),
        })
    }
}

impl<SignalId> ProcessingService<SignalId> {
    pub fn handle(&self) -> ProcessingHandle<SignalId> {
        self.handle.clone()
    }

    pub fn event_receiver(&self) -> Receiver<ProcessingEvent<SignalId>> {
        self.event_receiver.clone()
    }

    pub fn control_event_receiver(&self) -> Receiver<ControlEvent<SignalId>> {
        self.control_event_receiver.clone()
    }

    pub fn take_events(&self) -> Vec<ProcessingEvent<SignalId>> {
        self.event_receiver.try_iter().collect()
    }
}

impl<SignalId> Drop for ProcessingService<SignalId> {
    fn drop(&mut self) {
        let _ = self.handle.command_sender.send(ProcessingCommand::Shutdown);

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        AddControlLoopError, AddControllerDiagnosticError, AddSignalFilterError, ControlLoopState,
        ControllerRequestError, ProcessingEvent, ProcessingInput, ProcessingService,
        ProcessingServiceDisconnected, ReplaceSignalFilterError,
    };
    use crate::{
        connection::ConnectionId,
        instrument::{
            InstrumentValue, ParameterAccess, ParameterRange, ParameterValueType,
            virtual_instrument::{
                VirtualInstrumentId, VirtualParameterDescriptor, VirtualParameterId,
            },
        },
        process_control::{
            ControlEvent, ControlLoopDefinition, ControlOutput, ControlOutputTarget,
            ControllerAccessError, ControllerDiagnostic, PidController, PidGains, PidOutputLimits,
            ReferenceKind, ReferenceSource,
        },
        signal_processing::{
            ProcessedSignal, SignalFilterDefinition, SignalFilterError,
            SignalProcessingGraphDefinitionError, SignalProcessingGraphUpdateError,
        },
    };

    const EVENT_TIMEOUT: Duration = Duration::from_secs(1);

    const NO_EVENT_TIMEOUT: Duration = Duration::from_millis(50);

    fn pid_definition(
        name: &str,
        input: u64,
        parameter: u16,
    ) -> ControlLoopDefinition<u64, ControlOutputTarget> {
        let descriptor = VirtualParameterDescriptor::new(
            VirtualParameterId::new(parameter),
            format!("power_{parameter}"),
            "Power",
            ParameterAccess::ReadWrite,
            ParameterValueType::Number,
        )
        .with_range(ParameterRange::Number {
            minimum: 0.0,
            maximum: 100.0,
        });

        let target = ControlOutputTarget::virtual_instrument(
            ConnectionId::PRIMARY,
            VirtualInstrumentId::new(1),
            &descriptor,
        )
        .unwrap();

        let controller = PidController::with_output_limits(
            100.0,
            PidGains::new(2.0, 0.0, 0.0).unwrap(),
            PidOutputLimits::new(0.0, 100.0).unwrap(),
        )
        .unwrap()
        .into();

        ControlLoopDefinition::new(name, input, target, controller).unwrap()
    }

    fn ramp_pid_definition(
        name: &str,
        input: u64,
        parameter: u16,
    ) -> ControlLoopDefinition<u64, ControlOutputTarget> {
        pid_definition(name, input, parameter)
            .with_reference(ReferenceSource::ramp(20.0, 150.0, 10.0).unwrap())
    }

    fn receive_pid_output(
        events: &crossbeam_channel::Receiver<ControlEvent<u64>>,
    ) -> ControlOutput<u64> {
        let event = events.recv_timeout(EVENT_TIMEOUT).unwrap();

        let ControlEvent::Output(output) = event else {
            panic!("expected PID output event");
        };

        output
    }

    #[test]
    fn processes_signal_in_background_thread() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle.process(1, 0.0, 10.0).unwrap();

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(ProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 0.0,
                value: 10.0,
            },])),
        );

        handle.process(1, 1.0, 20.0).unwrap();

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(ProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 1.0,
                value: 15.0,
            },])),
        );
    }

    #[test]
    fn processes_input_batch() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .process_batch(vec![
                ProcessingInput::new(1, 0.0, 10.0),
                ProcessingInput::new(1, 1.0, 20.0),
            ])
            .unwrap();

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(ProcessingEvent::Samples(vec![
                ProcessedSignal {
                    signal_id: 2,
                    timestamp: 0.0,
                    value: 10.0,
                },
                ProcessedSignal {
                    signal_id: 2,
                    timestamp: 1.0,
                    value: 15.0,
                },
            ])),
        );
    }

    #[test]
    fn reports_filter_error_as_event() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle.process(1, 1.0, 10.0).unwrap();

        events.recv_timeout(EVENT_TIMEOUT).unwrap();

        handle.process(1, 1.0, 20.0).unwrap();

        let event = events.recv_timeout(EVENT_TIMEOUT).unwrap();

        let ProcessingEvent::Error(error) = event else {
            panic!("expected processing error event");
        };

        assert_eq!(error.output(), 2);

        assert_eq!(
            error.filter_error(),
            SignalFilterError::NonIncreasingTimestamp {
                previous: 1.0,
                current: 1.0,
            },
        );
    }

    #[test]
    fn rejects_duplicate_filter_output() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        assert_eq!(
            handle.add_filter(3, 2, SignalFilterDefinition::median(3).unwrap(),),
            Err(AddSignalFilterError::Definition(
                SignalProcessingGraphDefinitionError::DuplicateOutput { output: 2 },
            ),),
        );
    }

    #[test]
    fn reset_from_resets_filter_and_pid_state() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle.process(1, 10.0, 80.0).unwrap();

        let first_control = receive_pid_output(&control_events);

        assert_eq!(first_control.output.value(), 40.0,);

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(ProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 10.0,
                value: 80.0,
            },])),
        );

        handle.process(1, 11.0, 60.0).unwrap();

        receive_pid_output(&control_events);

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(ProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 11.0,
                value: 70.0,
            },])),
        );

        handle.reset_from(1).unwrap();

        handle.process(1, 0.0, 90.0).unwrap();

        let restarted_control = receive_pid_output(&control_events);

        assert_eq!(restarted_control.timestamp, 0.0,);

        assert_eq!(restarted_control.output.value(), 20.0,);

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(ProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 0.0,
                value: 90.0,
            },])),
        );
    }

    #[test]
    fn clear_removes_registered_processing_state() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle.clear().unwrap();

        handle.process(1, 0.0, 10.0).unwrap();

        assert!(events.recv_timeout(NO_EVENT_TIMEOUT).is_err(),);

        assert!(control_events.recv_timeout(NO_EVENT_TIMEOUT).is_err(),);
    }

    #[test]
    fn replaces_filter_in_background_service() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle.process(1, 0.0, 10.0).unwrap();

        events.recv_timeout(EVENT_TIMEOUT).unwrap();

        handle.process(1, 1.0, 20.0).unwrap();

        events.recv_timeout(EVENT_TIMEOUT).unwrap();

        handle
            .replace_filter(2, SignalFilterDefinition::median(3).unwrap())
            .unwrap();

        handle.process(1, 2.0, 100.0).unwrap();

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT),
            Ok(ProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 2,
                timestamp: 2.0,
                value: 100.0,
            },])),
        );
    }

    #[test]
    fn replacing_filter_resynchronizes_downstream_pid() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        /*
         * 1 -> filter 2 -> filter 3 -> PID
         *
         * Replacing filter 2 must also
         * resynchronize the PID attached
         * to downstream signal 3.
         */
        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_filter(2, 3, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("heater", 3, 1))
            .unwrap();

        /*
         * Isolate the derivative term.
         */
        handle
            .configure_controller(
                "heater",
                [
                    ("kp", InstrumentValue::Number(0.0)),
                    ("kd", InstrumentValue::Number(1.0)),
                ],
            )
            .unwrap();

        /*
         * Establish previous PID state at
         * measurement = 90.
         */
        handle.process(1, 0.0, 90.0).unwrap();

        let first = receive_pid_output(&control_events);

        assert_eq!(first.measurement, 90.0,);

        assert_eq!(first.output.derivative(), Some(0.0),);

        handle.process(1, 1.0, 90.0).unwrap();

        let second = receive_pid_output(&control_events);

        assert_eq!(second.measurement, 90.0,);

        assert_eq!(second.output.derivative(), Some(0.0),);

        /*
         * Replacing filter 2 resets both
         * filter 2 and downstream filter 3.
         */
        handle
            .replace_filter(2, SignalFilterDefinition::median(3).unwrap())
            .unwrap();

        /*
         * The new filter chain now outputs
         * 0 immediately.
         *
         * Without PID resynchronization the
         * derivative would see:
         *
         *     90 -> 0 in 1 second
         *
         * and produce a large artificial
         * kick.
         */
        handle.process(1, 2.0, 0.0).unwrap();

        let after_replacement = receive_pid_output(&control_events);

        assert_eq!(after_replacement.measurement, 0.0,);

        assert_eq!(after_replacement.output.derivative(), Some(0.0),);

        assert_eq!(after_replacement.output.value(), 0.0,);
    }

    #[test]
    fn replacing_filter_preserves_pid_integral() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_filter(2, 3, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("heater", 3, 1))
            .unwrap();

        /*
         * Isolate the integral term.
         */
        handle
            .configure_controller(
                "heater",
                [
                    ("kp", InstrumentValue::Number(0.0)),
                    ("ki", InstrumentValue::Number(1.0)),
                    ("kd", InstrumentValue::Number(0.0)),
                ],
            )
            .unwrap();

        /*
         * First sample establishes the
         * PID time base.
         */
        handle.process(1, 0.0, 90.0).unwrap();

        let first = receive_pid_output(&control_events);

        assert_eq!(first.output.integral(), Some(0.0),);

        /*
         * One second at error = 10:
         *
         * I = 1 * 10 * 1 = 10
         */
        handle.process(1, 1.0, 90.0).unwrap();

        let accumulated = receive_pid_output(&control_events);

        assert_eq!(accumulated.output.integral(), Some(10.0),);

        /*
         * Replace an upstream filter.
         *
         * This must discard PID timing
         * history, but preserve I = 10.
         */
        handle
            .replace_filter(2, SignalFilterDefinition::median(3).unwrap())
            .unwrap();

        /*
         * Both filters have fresh state,
         * so the chain immediately emits
         * 80.
         *
         * Because the PID was
         * resynchronized, this sample must
         * establish a new time base and
         * must not integrate the new error
         * yet.
         */
        handle.process(1, 2.0, 80.0).unwrap();

        let resumed = receive_pid_output(&control_events);

        assert_eq!(resumed.measurement, 80.0,);

        assert_eq!(resumed.output.integral(), Some(10.0),);

        assert_eq!(resumed.output.value(), 10.0,);

        /*
         * Normal integration resumes from
         * the following sample.
         *
         * error = 20
         * dt    = 1
         *
         * I = 10 + 20 = 30
         */
        handle.process(1, 3.0, 80.0).unwrap();

        let next = receive_pid_output(&control_events);

        assert_eq!(next.output.integral(), Some(30.0),);

        assert_eq!(next.output.value(), 30.0,);
    }

    #[test]
    fn rejects_replacing_unknown_service_filter() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        assert_eq!(
            handle.replace_filter(10, SignalFilterDefinition::median(3).unwrap(),),
            Err(ReplaceSignalFilterError::Definition(
                SignalProcessingGraphUpdateError::UnknownOutput { output: 10 },
            ),),
        );
    }

    #[test]
    fn runs_pid_for_raw_measurement() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle.process(1, 1_000.0, 80.0).unwrap();

        let output = receive_pid_output(&control_events);

        assert_eq!(output.loop_name, "heater",);

        assert_eq!(output.input, 1,);

        assert_eq!(output.measurement, 80.0,);

        assert_eq!(output.output.value(), 40.0,);
    }

    #[test]
    fn runs_pid_for_filtered_measurement() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("filtered_heater", 2, 1))
            .unwrap();

        handle.process(1, 1_000.0, 80.0).unwrap();

        let first = receive_pid_output(&control_events);

        handle.process(1, 1_001.0, 60.0).unwrap();

        let second = receive_pid_output(&control_events);

        assert_eq!(first.loop_name, "filtered_heater",);

        assert_eq!(first.input, 2,);

        assert_eq!(first.measurement, 80.0,);

        assert_eq!(first.output.value(), 40.0,);

        assert_eq!(second.loop_name, "filtered_heater",);

        assert_eq!(second.input, 2,);

        assert_eq!(second.measurement, 70.0,);

        assert_eq!(second.output.value(), 60.0,);
    }

    #[test]
    fn supports_raw_and_filtered_pid_loops_together() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_control_loop(pid_definition("raw_heater", 1, 1))
            .unwrap();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("filtered_heater", 2, 2))
            .unwrap();

        handle.process(1, 1_000.0, 80.0).unwrap();

        let first = receive_pid_output(&control_events);

        let second = receive_pid_output(&control_events);

        assert_eq!(first.loop_name, "raw_heater",);

        assert_eq!(first.input, 1,);

        assert_eq!(first.output.value(), 40.0,);

        assert_eq!(second.loop_name, "filtered_heater",);

        assert_eq!(second.input, 2,);

        assert_eq!(second.output.value(), 40.0,);
    }

    #[test]
    fn changes_controller_setpoint() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        assert_eq!(
            handle
                .write_controller_parameter("heater", "setpoint", InstrumentValue::Number(90.0,),),
            Ok(InstrumentValue::Number(90.0,),),
        );

        handle.process(1, 1_000.0, 80.0).unwrap();

        let output = receive_pid_output(&control_events);

        assert_eq!(output.output.setpoint(), Some(90.0),);

        assert_eq!(output.output.value(), 20.0,);
    }

    #[test]
    fn manages_reference_through_processing_handle() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_control_loop(ramp_pid_definition("heater", 1, 1))
            .unwrap();

        assert_eq!(
            handle.reference_kind("heater",),
            Ok(Some(ReferenceKind::Ramp,)),
        );

        let parameters = handle.reference_parameters("heater").unwrap();

        let keys = parameters
            .into_iter()
            .map(|parameter| parameter.key)
            .collect::<Vec<_>>();

        assert_eq!(keys, vec!["start", "target", "rate",],);

        assert_eq!(
            handle.read_reference_parameter("heater", "target",),
            Ok(InstrumentValue::Number(150.0,),),
        );

        handle.process(1, 10.0, 0.0).unwrap();

        let first = receive_pid_output(&control_events);

        assert_eq!(first.output.setpoint(), Some(20.0),);

        handle.process(1, 12.0, 0.0).unwrap();

        let second = receive_pid_output(&control_events);

        assert_eq!(second.output.setpoint(), Some(40.0),);

        assert_eq!(
            handle.write_reference_parameter("heater", "target", InstrumentValue::Number(200.0,),),
            Ok(InstrumentValue::Number(200.0,),),
        );

        assert_eq!(
            handle.read_controller_parameter("heater", "setpoint",),
            Ok(InstrumentValue::Number(40.0,),),
        );

        handle.process(1, 1_000.0, 0.0).unwrap();

        let restarted = receive_pid_output(&control_events);

        assert_eq!(restarted.output.setpoint(), Some(40.0),);

        handle.process(1, 1_001.0, 0.0).unwrap();

        let next = receive_pid_output(&control_events);

        assert_eq!(next.output.setpoint(), Some(50.0),);
    }

    #[test]
    fn rejects_duplicate_pid_name() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        assert!(matches!(
            handle.add_control_loop(pid_definition("heater", 2, 2,),),
            Err(AddControlLoopError::Definition(_)),
        ));
    }

    #[test]
    fn configures_reference_through_processing_handle() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(ramp_pid_definition("heater", 1, 1))
            .unwrap();

        handle
            .configure_reference(
                "heater",
                [
                    ("target", InstrumentValue::Number(200.0)),
                    ("rate", InstrumentValue::Number(5.0)),
                ],
            )
            .unwrap();

        assert_eq!(
            handle.read_reference_parameter("heater", "target",),
            Ok(InstrumentValue::Number(200.0,),),
        );

        assert_eq!(
            handle.read_reference_parameter("heater", "rate",),
            Ok(InstrumentValue::Number(5.0,),),
        );
    }

    #[test]
    fn replaces_reference_through_processing_handle() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(ramp_pid_definition("heater", 1, 1))
            .unwrap();

        handle
            .set_reference("heater", ReferenceSource::fixed(175.0).unwrap())
            .unwrap();

        assert_eq!(
            handle.reference_kind("heater",),
            Ok(Some(ReferenceKind::Fixed,)),
        );

        assert_eq!(
            handle.read_reference_parameter("heater", "value",),
            Ok(InstrumentValue::Number(175.0,),),
        );

        assert_eq!(
            handle.read_controller_parameter("heater", "setpoint",),
            Ok(InstrumentValue::Number(175.0,),),
        );
    }

    #[test]
    fn remove_from_removes_raw_and_dependent_pid_loops() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("raw_heater", 1, 1))
            .unwrap();

        handle
            .add_control_loop(pid_definition("filtered_heater", 2, 2))
            .unwrap();

        let removed = handle.remove_from(1).unwrap();

        assert!(
            removed.contains(&2),
            "filtered output must be removed with its input",
        );

        handle.process(1, 1_000.0, 80.0).unwrap();

        assert!(
            control_events.recv_timeout(NO_EVENT_TIMEOUT).is_err(),
            "PID loops for the removed branch must no longer run",
        );
    }

    #[test]
    fn remove_from_does_not_remove_unrelated_pid_loop() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("removed_heater", 2, 1))
            .unwrap();

        handle
            .add_control_loop(pid_definition("unrelated_heater", 3, 2))
            .unwrap();

        handle.remove_from(1).unwrap();

        handle.process(3, 1_000.0, 80.0).unwrap();

        let output = receive_pid_output(&control_events);

        assert_eq!(output.loop_name, "unrelated_heater",);

        assert_eq!(output.input, 3,);

        assert_eq!(output.output.value(), 40.0,);

        assert!(control_events.recv_timeout(NO_EVENT_TIMEOUT).is_err(),);
    }

    #[test]
    fn accepts_empty_batch() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        assert_eq!(service.handle().process_batch(Vec::new()), Ok(()),);
    }

    #[test]
    fn cloned_handle_reports_stopped_service() {
        let handle = {
            let service = ProcessingService::<u64>::spawn().unwrap();

            service.handle()
        };

        assert_eq!(
            handle.process(1, 1_000.0, 80.0,),
            Err(ProcessingServiceDisconnected),
        );

        assert_eq!(
            handle.add_control_loop(pid_definition("heater", 1, 1,),),
            Err(AddControlLoopError::Disconnected),
        );
    }

    #[test]
    fn describes_service_errors() {
        assert_eq!(
            AddSignalFilterError::<u64>::Disconnected.to_string(),
            "Processing service is disconnected",
        );

        assert_eq!(
            AddControlLoopError::Disconnected.to_string(),
            "Processing service is disconnected",
        );

        assert_eq!(
            ProcessingServiceDisconnected.to_string(),
            "Processing service is disconnected",
        );

        assert_eq!(
            ReplaceSignalFilterError::<u64>::Disconnected.to_string(),
            "Processing service is disconnected",
        );
    }

    #[test]
    fn reads_controller_parameter() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        assert_eq!(
            handle.read_controller_parameter("heater", "setpoint",),
            Ok(InstrumentValue::Number(100.0,),),
        );
    }

    #[test]
    fn writes_controller_parameter() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        assert_eq!(
            handle.write_controller_parameter(
                "heater",
                "setpoint",
                InstrumentValue::Number(120.0,),
            ),
            Ok(InstrumentValue::Number(120.0,),),
        );

        assert_eq!(
            handle.read_controller_parameter("heater", "setpoint",),
            Ok(InstrumentValue::Number(120.0,),),
        );
    }

    #[test]
    fn configures_controller_atomically() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle
            .configure_controller(
                "heater",
                [
                    ("output_min", InstrumentValue::Number(10.0)),
                    ("output_max", InstrumentValue::Number(90.0)),
                ],
            )
            .unwrap();

        assert_eq!(
            handle.read_controller_parameter("heater", "output_min",),
            Ok(InstrumentValue::Number(10.0)),
        );

        assert_eq!(
            handle.read_controller_parameter("heater", "output_max",),
            Ok(InstrumentValue::Number(90.0)),
        );
    }

    #[test]
    fn reports_missing_controller_request() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        assert!(matches!(
            service
                .handle()
                .read_controller_parameter("missing", "setpoint",),
            Err(ControllerRequestError::Access(_)),
        ));
    }

    #[test]
    fn resets_controller_by_name() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle
            .write_controller_parameter("heater", "ki", InstrumentValue::Number(1.0))
            .unwrap();

        handle.process(1, 0.0, 90.0).unwrap();

        let _ = receive_pid_output(&control_events);

        handle.process(1, 1.0, 90.0).unwrap();

        let accumulated = receive_pid_output(&control_events);

        assert!(accumulated.output.integral().unwrap() > 0.0,);

        handle.reset_controller("heater").unwrap();

        handle.process(1, 0.0, 90.0).unwrap();

        let restarted = receive_pid_output(&control_events);

        assert_eq!(restarted.output.integral(), Some(0.0),);
    }

    #[test]
    fn emits_controller_diagnostic_sample() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let events = service.event_receiver();

        let control_events = service.control_event_receiver();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle
            .add_controller_diagnostic("heater", ControllerDiagnostic::Proportional, 10)
            .unwrap();

        handle.process(1, 1_000.0, 80.0).unwrap();

        let control = receive_pid_output(&control_events);

        assert_eq!(control.output.proportional(), Some(40.0),);

        assert_eq!(
            events.recv_timeout(EVENT_TIMEOUT,),
            Ok(ProcessingEvent::Samples(vec![ProcessedSignal {
                signal_id: 10,
                timestamp: 1_000.0,
                value: 40.0,
            },],),),
        );
    }

    #[test]
    fn rejects_diagnostic_for_missing_controller() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        assert_eq!(
            handle.add_controller_diagnostic("missing", ControllerDiagnostic::Integral, 10,),
            Err(AddControllerDiagnosticError::Controller(
                ControllerAccessError::ControlLoopNotFound("missing".to_owned(),),
            )),
        );
    }

    #[test]
    fn removes_diagnostics_with_controller_input() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle
            .add_controller_diagnostic("heater", ControllerDiagnostic::Integral, 10)
            .unwrap();

        assert_eq!(handle.remove_from(1), Ok(vec![10]),);
    }

    #[test]
    fn removing_controller_detaches_diagnostics_and_preserves_input_filter() {
        let service = ProcessingService::<u64>::spawn().unwrap();
        let handle = service.handle();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();
        handle
            .add_control_loop(pid_definition("heater", 2, 1))
            .unwrap();
        handle
            .add_controller_diagnostic("heater".to_owned(), ControllerDiagnostic::Output, 3)
            .unwrap();

        assert!(handle.remove_controller("heater").unwrap());
        assert!(!handle.remove_controller("heater").unwrap());

        handle
            .add_control_loop(pid_definition("heater", 2, 1))
            .unwrap();
        handle.process(1, 1000.0, 80.0).unwrap();

        assert_eq!(
            receive_pid_output(&service.control_event_receiver()).input,
            2
        );

        let ProcessingEvent::Samples(samples) = service
            .event_receiver()
            .recv_timeout(EVENT_TIMEOUT)
            .unwrap()
        else {
            panic!("expected samples");
        };

        assert!(samples.iter().any(|sample| sample.signal_id == 2));
        assert!(samples.iter().all(|sample| sample.signal_id != 3));
    }

    #[test]
    fn pauses_and_resumes_controller_without_losing_state() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();
        let control_events = service.control_event_receiver();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle
            .write_controller_parameter("heater", "ki", InstrumentValue::Number(1.0))
            .unwrap();

        handle.process(1, 0.0, 90.0).unwrap();
        let _ = receive_pid_output(&control_events);

        handle.process(1, 1.0, 90.0).unwrap();

        let accumulated = receive_pid_output(&control_events);

        assert_eq!(accumulated.output.integral(), Some(10.0),);

        handle.pause_controller("heater").unwrap();

        assert_eq!(
            handle.controller_state("heater"),
            Ok(ControlLoopState::Paused),
        );

        handle.process(1, 100.0, 90.0).unwrap();

        assert!(matches!(
            control_events.recv_timeout(NO_EVENT_TIMEOUT),
            Err(crossbeam_channel::RecvTimeoutError::Timeout),
        ));

        handle.resume_controller("heater").unwrap();

        assert_eq!(
            handle.controller_state("heater"),
            Ok(ControlLoopState::Running),
        );

        handle.process(1, 101.0, 90.0).unwrap();

        let resumed = receive_pid_output(&control_events);

        assert_eq!(resumed.output.integral(), Some(10.0),);
    }

    #[test]
    fn changes_controller_input_without_stopping_processing() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        let control_events = service.control_event_receiver();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("heater", 1, 1))
            .unwrap();

        handle
            .write_controller_parameter("heater", "ki", InstrumentValue::Number(1.0))
            .unwrap();

        handle.process(1, 0.0, 90.0).unwrap();

        let _ = receive_pid_output(&control_events);

        handle.process(1, 1.0, 90.0).unwrap();

        let accumulated = receive_pid_output(&control_events);

        assert_eq!(accumulated.input, 1,);

        assert!(accumulated.output.integral().unwrap() > 0.0);

        let integral = accumulated.output.integral().unwrap();

        handle.set_controller_input("heater", 2).unwrap();

        handle.process(1, 100.0, 90.0).unwrap();

        let filtered = receive_pid_output(&control_events);

        assert_eq!(filtered.input, 2,);

        assert_eq!(filtered.output.integral(), Some(integral),);

        assert_eq!(filtered.output.derivative(), Some(0.0),);
    }

    #[test]
    fn previews_controllers_removed_with_signal_branch() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(pid_definition("raw_heater", 1, 1))
            .unwrap();

        handle
            .add_filter(1, 2, SignalFilterDefinition::moving_average(2).unwrap())
            .unwrap();

        handle
            .add_control_loop(pid_definition("filtered_heater", 2, 2))
            .unwrap();

        handle
            .add_control_loop(pid_definition("unrelated_heater", 3, 3))
            .unwrap();

        assert_eq!(
            handle.controllers_affected_by_removal(1,).unwrap(),
            vec!["raw_heater".to_owned(), "filtered_heater".to_owned(),],
        );

        /*
         * Preview must not mutate runtime.
         */
        assert_eq!(
            handle.controller_state("raw_heater",),
            Ok(ControlLoopState::Running),
        );

        assert_eq!(
            handle.controller_state("filtered_heater",),
            Ok(ControlLoopState::Running),
        );

        assert_eq!(
            handle.controller_state("unrelated_heater",),
            Ok(ControlLoopState::Running),
        );
    }

    #[test]
    fn returns_all_controller_names_without_modifying_runtime() {
        let service = ProcessingService::<u64>::spawn().unwrap();

        let handle = service.handle();

        handle
            .add_control_loop(pid_definition("first", 1, 1))
            .unwrap();

        handle
            .add_control_loop(pid_definition("second", 2, 2))
            .unwrap();

        assert_eq!(
            handle.controller_names().unwrap(),
            vec!["first".to_owned(), "second".to_owned(),],
        );

        assert_eq!(
            handle.controller_state("first",),
            Ok(ControlLoopState::Running,),
        );

        assert_eq!(
            handle.controller_state("second",),
            Ok(ControlLoopState::Running,),
        );
    }
}
