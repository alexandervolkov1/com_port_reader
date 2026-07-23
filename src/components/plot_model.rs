use egui_plot::PlotPoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlotPaneId(u64);

impl PlotPaneId {
    const fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Default)]
pub struct PlotLine {
    pub name: String,
    pub points: Vec<PlotPoint>,
}

pub struct PlotPane {
    pub id: PlotPaneId,
    pub lines: Vec<PlotLine>,
}

impl PlotPane {
    fn new(id: PlotPaneId) -> Self {
        Self {
            id,
            lines: Vec::new(),
        }
    }
}

pub struct PlotModel {
    pub follow_latest: bool,
    pub manual_x_bounds: Option<(f64, f64)>,
    pub panes: Vec<PlotPane>,
}

impl PlotModel {
    pub fn new() -> Self {
        Self {
            follow_latest: true,
            manual_x_bounds: None,
            panes: vec![PlotPane::new(PlotPaneId::new(1))],
        }
    }
}

impl Default for PlotModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::PlotModel;

    #[test]
    fn starts_with_one_plot_pane() {
        let plot = PlotModel::new();

        assert_eq!(plot.panes.len(), 1);
        assert!(plot.panes[0].lines.is_empty());
    }
}
