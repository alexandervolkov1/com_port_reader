use crossbeam_channel::Sender;

use crate::{
    acquisition::{InstrumentReadResult, InstrumentWriteResult, VirtualInstrumentDescribeResult},
    data::NewFilteredSeries,
    instrument::{InstrumentReadRequest, InstrumentWriteRequest},
    process_control::{ControlOutputTarget, NewPidLoop},
    serial_connection::SerialPortConfig,
    signal_processing::SignalFilterDefinition,
};

pub enum WorkerCommand {
    Start,
    Stop,
    AddFilter(NewFilteredSeries),
    AddPidLoop(NewPidLoop<ControlOutputTarget>),
    SetPidSetpoint {
        name: String,
        setpoint: f64,
    },
    SetFilter {
        name: String,
        definition: SignalFilterDefinition,
    },
    ClearSeries,
    Shutdown,
    RemoveSeriesByName(String),
    Connection(ConnectionCommand),
    RefreshSeriesSchedule,
}

pub enum ConnectionCommand {
    SendSerialText {
        config: SerialPortConfig,
        command: String,
    },

    ReadInstrument {
        port_name: String,
        request: InstrumentReadRequest,
        response_sender: Sender<InstrumentReadResult>,
    },

    WriteInstrument {
        port_name: String,
        request: InstrumentWriteRequest,
        emit_event: bool,
        response_sender: Sender<InstrumentWriteResult>,
    },

    DescribeVirtualInstruments {
        response_sender: Sender<VirtualInstrumentDescribeResult>,
    },
}
