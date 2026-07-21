use egui_plot::PlotPoint;

#[derive(Default)]
pub struct PlotLine {
    pub name: String,
    pub points: Vec<PlotPoint>,
}

pub struct PlotModel {
    pub follow_latest: bool,
    pub last_plot_x: f64,
    pub lines: Vec<PlotLine>,
}

impl PlotModel {
    pub fn new() -> Self {
        Self {
            follow_latest: true,
            last_plot_x: 0.0,
            lines: Vec::new(),
        }
    }
}
