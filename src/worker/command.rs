use std::path::PathBuf;

use crate::data::{NewSeries, SeriesId};
use crate::serial_connection::SerialPortConfig;

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
    TestSerialPort(SerialPortConfig),
    TestSerialCommand {
        config: SerialPortConfig,
        command: String,
    },
}
