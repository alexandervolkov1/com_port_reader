use std::collections::HashMap;

use egui_plot::PlotPoint;

use crate::data::SeriesId;

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
    pub series_panes: HashMap<SeriesId, PlotPaneId>,
    next_pane_id: u64,
}

impl PlotModel {
    pub fn new() -> Self {
        Self {
            follow_latest: true,
            manual_x_bounds: None,
            panes: vec![PlotPane::new(PlotPaneId::new(1))],
            series_panes: HashMap::new(),
            next_pane_id: 2,
        }
    }

    pub fn add_pane(&mut self) {
        let id = PlotPaneId::new(self.next_pane_id);

        self.next_pane_id += 1;

        self.panes.push(PlotPane::new(id));
    }

    pub fn remove_last_pane(&mut self) {
        if self.panes.len() <= 1 {
            return;
        }

        let removed_pane = self.panes.pop().expect("more than one pane exists");

        self.series_panes
            .retain(|_, pane_id| *pane_id != removed_pane.id);
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

    #[test]
    fn adds_and_removes_plot_pane() {
        let mut plot = PlotModel::new();

        plot.add_pane();

        assert_eq!(plot.panes.len(), 2);

        plot.remove_last_pane();

        assert_eq!(plot.panes.len(), 1);
    }

    #[test]
    fn does_not_remove_last_plot_pane() {
        let mut plot = PlotModel::new();

        plot.remove_last_pane();

        assert_eq!(plot.panes.len(), 1);
    }
}
