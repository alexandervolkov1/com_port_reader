use std::collections::HashMap;

use egui_plot::PlotPoint;

use crate::data::{SeriesColor, SeriesId};

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
    pub color: Option<SeriesColor>,
}

pub struct PlotPane {
    pub id: PlotPaneId,
    pub lines: Vec<PlotLine>,
    pub auto_y: bool,
    pub height_weight: f32,
}

impl PlotPane {
    fn new(id: PlotPaneId, height_weight: f32) -> Self {
        Self {
            id,
            lines: Vec::new(),
            auto_y: true,
            height_weight,
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
            panes: vec![PlotPane::new(PlotPaneId::new(1), 1.0)],
            series_panes: HashMap::new(),
            next_pane_id: 2,
        }
    }

    pub fn add_pane(&mut self) {
        let id = PlotPaneId::new(self.next_pane_id);

        self.next_pane_id += 1;

        let new_weight = {
            let last_pane = self
                .panes
                .last_mut()
                .expect("plot model always has one pane");

            let new_weight = last_pane.height_weight * 0.5;

            last_pane.height_weight -= new_weight;

            new_weight
        };

        self.panes.push(PlotPane::new(id, new_weight));
    }

    pub fn remove_last_pane(&mut self) {
        if self.panes.len() <= 1 {
            return;
        }

        let removed_pane = self.panes.pop().expect("more than one pane exists");

        self.panes
            .last_mut()
            .expect("at least one pane remains")
            .height_weight += removed_pane.height_weight;

        self.series_panes
            .retain(|_, pane_id| *pane_id != removed_pane.id);
    }

    pub fn resize_adjacent_panes(&mut self, upper_index: usize, delta_weight: f32) {
        if !delta_weight.is_finite() || upper_index + 1 >= self.panes.len() {
            return;
        }

        let (upper_panes, lower_panes) = self.panes.split_at_mut(upper_index + 1);

        let upper = &mut upper_panes[upper_index];
        let lower = &mut lower_panes[0];

        let combined_weight = upper.height_weight + lower.height_weight;

        let upper_weight = (upper.height_weight + delta_weight).clamp(0.0, combined_weight);

        upper.height_weight = upper_weight;
        lower.height_weight = combined_weight - upper_weight;
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
        assert_eq!(plot.panes[0].height_weight, 1.0);
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

    #[test]
    fn splits_last_pane_weight_when_adding() {
        let mut plot = PlotModel::new();

        plot.add_pane();

        assert_eq!(plot.panes[0].height_weight, 0.5);
        assert_eq!(plot.panes[1].height_weight, 0.5);

        plot.add_pane();

        assert_eq!(plot.panes[0].height_weight, 0.5);
        assert_eq!(plot.panes[1].height_weight, 0.25);
        assert_eq!(plot.panes[2].height_weight, 0.25);
    }

    #[test]
    fn returns_removed_weight_to_previous_pane() {
        let mut plot = PlotModel::new();

        plot.add_pane();
        plot.add_pane();
        plot.remove_last_pane();

        assert_eq!(plot.panes[0].height_weight, 0.5);
        assert_eq!(plot.panes[1].height_weight, 0.5);
    }

    #[test]
    fn resizes_adjacent_panes() {
        let mut plot = PlotModel::new();

        plot.add_pane();

        plot.resize_adjacent_panes(0, 0.2);

        assert!((plot.panes[0].height_weight - 0.7).abs() < f32::EPSILON,);

        assert!((plot.panes[1].height_weight - 0.3).abs() < f32::EPSILON,);

        plot.resize_adjacent_panes(0, 10.0);

        assert_eq!(plot.panes[0].height_weight, 1.0);
        assert_eq!(plot.panes[1].height_weight, 0.0);
    }
}
