use crossbeam_channel::Sender;

use crate::{
    acquisition::{InstrumentReadResult, InstrumentWriteResult, VirtualInstrumentDescribeResult},
    connection::ConnectionId,
    data::{NewFilteredSeries, NewSeries, SeriesColor},
    instrument::{
        InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest, ParameterDescriptor,
    },
    process_control::{ControlOutputTarget, NewPidLoop},
    signal_processing::{ControllerRequestError, SignalFilterDefinition},
};

#[derive(Debug)]
pub enum UserCommand {
    Add(NewSeries),
    AddFilter(NewFilteredSeries),
    AddPidLoop(NewPidLoop<ControlOutputTarget>),

    ControllerParameters {
        name: String,
        response_sender: Sender<Result<Vec<ParameterDescriptor>, ControllerRequestError>>,
    },

    ReadControllerParameter {
        name: String,
        key: String,
        response_sender: Sender<Result<InstrumentValue, ControllerRequestError>>,
    },

    WriteControllerParameter {
        name: String,
        key: String,
        value: InstrumentValue,
        response_sender: Sender<Result<InstrumentValue, ControllerRequestError>>,
    },

    ConfigureController {
        name: String,
        updates: Vec<(String, InstrumentValue)>,
        response_sender: Sender<Result<(), ControllerRequestError>>,
    },

    ResetController {
        name: String,
        response_sender: Sender<Result<(), ControllerRequestError>>,
    },

    SetFilter {
        name: String,
        definition: SignalFilterDefinition,
    },

    Delete {
        name: String,
    },

    Rename {
        current_name: String,
        new_name: String,
    },

    SetSeriesColor {
        name: String,
        color: Option<SeriesColor>,
    },

    Retry {
        name: String,
    },

    RetryAll,

    Start,
    Stop,
    Clear,

    StartEmulator,
    StopEmulator,

    Log {
        message: String,
    },

    SendSerial {
        connection_id: ConnectionId,
        command: String,
    },

    ReadInstrument {
        connection_id: ConnectionId,
        request: InstrumentReadRequest,
        response_sender: Sender<InstrumentReadResult>,
    },

    WriteInstrument {
        connection_id: ConnectionId,
        request: InstrumentWriteRequest,
        response_sender: Sender<InstrumentWriteResult>,
    },

    DescribeVirtualInstruments {
        connection_id: ConnectionId,
        response_sender: Sender<VirtualInstrumentDescribeResult>,
    },
}
