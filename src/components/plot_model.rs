use egui_plot::PlotPoint;

pub struct PlotModel {
    pub follow_latest: bool,
    pub last_plot_x: f64,
    pub plot_cache: Vec<Vec<PlotPoint>>,
}

impl PlotModel {
    pub fn new() -> Self {
        Self {
            follow_latest: true,
            last_plot_x: 0.0,
            plot_cache: Vec::new(),
        }
    }
}
