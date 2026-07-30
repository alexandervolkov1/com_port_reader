use crate::data::NewSeries;
use crate::protocol::metakon::WriteRegisterRequest;

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

    SendSerial {
        command: String,
    },

    WriteMetakon {
        request: WriteRegisterRequest,
    },
}
