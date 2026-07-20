use egui_plot::PlotPoint;

use super::Signal;

#[derive(Clone)]
pub struct SignalSeries {
    pub signal: Signal,
    pub points: Vec<PlotPoint>,
    pub visible: bool,
}
