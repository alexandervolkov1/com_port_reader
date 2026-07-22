use crate::data::{NewSeries, SeriesId};

pub enum WorkerCommand {
    Start,
    Stop,

    AddSeries(NewSeries),
    RemoveSeries(SeriesId),
    SetVisibility { id: SeriesId, visible: bool },
    ClearSeries,
    Shutdown,
    RemoveSeriesByName(String),
}
