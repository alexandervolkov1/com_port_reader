use super::{Sample, SeriesId};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeriesSample {
    pub series_id: SeriesId,
    pub sample: Sample,
}

impl SeriesSample {
    pub const fn new(series_id: SeriesId, sample: Sample) -> Self {
        Self { series_id, sample }
    }
}
