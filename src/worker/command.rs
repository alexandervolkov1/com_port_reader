use std::path::PathBuf;

use crossbeam_channel::Sender;

use crate::{
    acquisition::{InstrumentReadResult, InstrumentWriteResult, VirtualInstrumentDescribeResult},
    data::{NewSeries, SeriesId},
    instrument::{InstrumentReadRequest, InstrumentWriteRequest},
    serial_connection::SerialPortConfig,
};

pub enum WorkerCommand {
    Start,
    Stop,
    AddSeries(NewSeries),
    RemoveSeries(SeriesId),
    SetVisibility {
        id: SeriesId,
        visible: bool,
    },
    ClearSeries,
    Shutdown,
    RemoveSeriesByName(String),
    RenameSeries {
        current_name: String,
        new_name: String,
    },
    StartCsvRecording(PathBuf),
    StopRecording,
    Connection(ConnectionCommand),
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
        response_sender: Sender<InstrumentWriteResult>,
    },

    DescribeVirtualInstruments {
        response_sender: Sender<VirtualInstrumentDescribeResult>,
    },
}
