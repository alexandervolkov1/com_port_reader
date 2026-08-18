use crossbeam_channel::Sender;

use crate::{
    acquisition::{InstrumentReadResult, InstrumentWriteResult, VirtualInstrumentDescribeResult},
    connection::ConnectionId,
    data::{NewSeries, SeriesColor},
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
