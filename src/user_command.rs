use crossbeam_channel::Sender;

use crate::{
    acquisition::{InstrumentReadResult, InstrumentWriteResult, VirtualInstrumentDescribeResult},
    connection::ConnectionId,
    data::NewSeries,
    instrument::{InstrumentReadRequest, InstrumentWriteRequest},
};

#[derive(Debug)]
pub enum UserCommand {
    Add(NewSeries),
    Delete {
        name: String,
    },
    Rename {
        current_name: String,
        new_name: String,
    },
    Retry {
        name: String,
    },
    RetryAll,
    Start,
    Stop,
    Clear,

    StartRecording,
    StopRecording,

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
