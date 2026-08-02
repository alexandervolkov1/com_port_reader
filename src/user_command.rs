use crossbeam_channel::Sender;

use crate::{
    acquisition::InstrumentReadResult, data::NewSeries, instrument::InstrumentReadRequest,
    protocol::metakon::WriteRegisterRequest,
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
        command: String,
    },

    ReadInstrument {
        request: InstrumentReadRequest,
        response_sender: Sender<InstrumentReadResult>,
    },

    WriteMetakon {
        request: WriteRegisterRequest,
    },
}
