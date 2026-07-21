use crate::data::{SeriesId, Signal};

pub enum WorkerCommand {
    Start,
    Stop,

    AddSignal(Signal),
    RemoveSeries(SeriesId),
    SetVisibility { id: SeriesId, visible: bool },
    ClearSeries,

    Shutdown,
}
