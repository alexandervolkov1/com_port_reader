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
    pub auto_y: bool,
}

impl PlotPane {
    fn new(id: PlotPaneId) -> Self {
        Self {
            id,
            lines: Vec::new(),
            auto_y: true,
        }
    }
}

pub struct PlotModel {
    pub follow_latest: bool,
    pub manual_x_bounds: Option<(f64, f64)>,
    pub panes: Vec<PlotPane>,
    pub(crate) series_panes: HashMap<SeriesId, PlotPaneId>,
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

    pub fn pane_for_series(&self, series_id: SeriesId) -> PlotPaneId {
        self.series_panes
            .get(&series_id)
            .copied()
            .unwrap_or(self.panes[0].id)
    }

    pub fn assign_series(&mut self, series_id: SeriesId, pane_id: PlotPaneId) {
        if !self.panes.iter().any(|pane| pane.id == pane_id) {
            return;
        }

        let default_pane_id = self.panes[0].id;

        if pane_id == default_pane_id {
            self.series_panes.remove(&series_id);
        } else {
            self.series_panes.insert(series_id, pane_id);
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
    use crate::data::SeriesId;

    #[test]
    fn starts_with_one_plot_pane() {
        let plot = PlotModel::new();

        assert_eq!(plot.panes.len(), 1);
        assert!(plot.panes[0].lines.is_empty());
        assert!(plot.panes[0].auto_y);
    }

    #[test]
    fn adds_and_removes_plot_pane() {
        let mut plot = PlotModel::new();

        plot.add_pane();

        assert!(plot.panes[1].auto_y);

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

    #[test]
    fn assigns_series_to_plot_pane() {
        let mut plot = PlotModel::new();
        let series_id = SeriesId::new(42);

        plot.add_pane();

        let second_pane_id = plot.panes[1].id;

        plot.assign_series(series_id, second_pane_id);

        assert_eq!(plot.pane_for_series(series_id), second_pane_id,);
    }

    #[test]
    fn returns_series_to_first_pane_when_removed() {
        let mut plot = PlotModel::new();
        let series_id = SeriesId::new(42);

        plot.add_pane();

        let first_pane_id = plot.panes[0].id;

        let second_pane_id = plot.panes[1].id;

        plot.assign_series(series_id, second_pane_id);

        plot.remove_last_pane();

        assert_eq!(plot.pane_for_series(series_id), first_pane_id,);
    }
}
