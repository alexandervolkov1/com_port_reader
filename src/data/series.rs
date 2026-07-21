use egui_plot::PlotPoint;

use super::Signal;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SeriesId(u64);

impl SeriesId {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone)]
pub struct SignalSeries {
    pub id: SeriesId,
    pub signal: Signal,
    pub points: Vec<PlotPoint>,
    pub visible: bool,
}

impl SignalSeries {
    pub(crate) fn new(id: SeriesId, signal: Signal) -> Self {
        Self {
            id,
            signal,
            points: Vec::new(),
            visible: true,
        }
    }
}
