use crossbeam_channel::Sender;

use crate::{
    acquisition::{
        InstrumentReadResult, InstrumentWriteCompletion, InstrumentWriteCompletionId,
        InstrumentWriteResult, VirtualInstrumentDescribeResult,
    },
    instrument::{InstrumentReadRequest, InstrumentWriteRequest},
    serial_connection::SerialPortConfig,
};

pub enum WorkerCommand {
    Start,
    Stop,
    Shutdown,
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
        completion: Option<(
            InstrumentWriteCompletionId,
            Sender<InstrumentWriteCompletion>,
        )>,
        response_sender: Sender<InstrumentWriteResult>,
    },

    DescribeVirtualInstruments {
        response_sender: Sender<VirtualInstrumentDescribeResult>,
    },
}
