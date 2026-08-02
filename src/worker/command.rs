use std::{path::PathBuf, time::Duration};

use crossbeam_channel::Sender;

use crate::{
    acquisition::InstrumentReadResult,
    data::{NewSeries, SeriesId},
    instrument::InstrumentReadRequest,
    protocol::metakon::WriteRegisterRequest,
    serial_connection::SerialPortConfig,
};

pub enum WorkerCommand {
    Start,
    Stop,
    SetPollInterval(Duration),
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
    TestSerialPort(SerialPortConfig),
    SendSerialText {
        config: SerialPortConfig,
        command: String,
    },

    ReadInstrument {
        port_name: String,
        request: InstrumentReadRequest,
        response_sender: Sender<InstrumentReadResult>,
    },

    WriteMetakon {
        config: SerialPortConfig,
        request: WriteRegisterRequest,
    },
}
