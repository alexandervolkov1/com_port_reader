use egui_plot::PlotPoint;

#[derive(Default)]
pub struct PlotLine {
    pub name: String,
    pub points: Vec<PlotPoint>,
}

pub struct PlotModel {
    pub follow_latest: bool,
    pub manual_x_bounds: Option<(f64, f64)>,
    pub lines: Vec<PlotLine>,
}

impl PlotModel {
    pub fn new() -> Self {
        Self {
            follow_latest: true,
            manual_x_bounds: None,
            lines: Vec::new(),
        }
    }
}
