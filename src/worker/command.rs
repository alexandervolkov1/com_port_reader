use crossbeam_channel::Sender;

use crate::{
    acquisition::{
        InstrumentReadResult, InstrumentWriteCompletion, InstrumentWriteCompletionId,
        InstrumentWriteResult, VirtualInstrumentDescribeResult,
    },
    instrument::{InstrumentReadRequest, InstrumentWriteRequest},
    process_recorder::ProcessActionId,
    serial_connection::SerialPortConfig,
};

pub enum WorkerCommand {
    Start { action_id: Option<ProcessActionId> },
    Stop { action_id: Option<ProcessActionId> },
    Shutdown,
    Connection(ConnectionCommand),
    RefreshSeriesSchedule,
}

pub enum ConnectionCommand {
    SendSerialText {
        action_id: Option<ProcessActionId>,
        config: SerialPortConfig,
        command: String,
    },

    ReadInstrument {
        action_id: Option<ProcessActionId>,
        port_name: String,
        request: InstrumentReadRequest,
        response_sender: Sender<InstrumentReadResult>,
    },

    WriteInstrument {
        action_id: Option<ProcessActionId>,
        port_name: String,
        request: InstrumentWriteRequest,
        emit_event: bool,
        completion: Option<(
            InstrumentWriteCompletionId,
            Sender<InstrumentWriteCompletion>,
        )>,
        response_sender: Sender<InstrumentWriteResult>,
    },

    DescribeVirtualInstruments {
        action_id: Option<ProcessActionId>,
        response_sender: Sender<VirtualInstrumentDescribeResult>,
    },
}
