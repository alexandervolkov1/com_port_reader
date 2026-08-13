use eframe::egui;

use super::control_panel_model::{ControlPanelModel, ControlState};

const VIEWPORT_ID: &str = "application_control_panel";

pub fn show_menu_button(ui: &mut egui::Ui, model: &ControlPanelModel, open: &mut bool) {
    let configured = !model.panels().is_empty();

    let response = ui.add_enabled(configured, egui::Button::new("Control panel"));

    if response.clicked() {
        *open = !*open;
    }

    if !configured {
        response.on_disabled_hover_text("No control panels are defined in startup.lua");
    }
}

pub fn show_viewport(root_ui: &mut egui::Ui, model: &ControlPanelModel, open: &mut bool) {
    if !*open {
        return;
    }

    root_ui.ctx().show_viewport_immediate(
        egui::ViewportId::from_hash_of(VIEWPORT_ID),
        egui::ViewportBuilder::default()
            .with_title("Control panel")
            .with_inner_size([360.0, 500.0])
            .with_min_inner_size([280.0, 240.0]),
        |ui, viewport_class| {
            if viewport_class == egui::ViewportClass::EmbeddedWindow {
                ui.label(
                    "This backend does not support \
                     separate native windows.",
                );

                return;
            }

            egui::CentralPanel::default().show(ui, |ui| {
                if ui.input(|input| input.viewport().close_requested()) {
                    *open = false;
                }

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        show_contents(ui, model);
                    });
            });
        },
    );
}

fn show_contents(ui: &mut egui::Ui, model: &ControlPanelModel) {
    for (panel_index, panel) in model.panels().iter().enumerate() {
        if panel_index > 0 {
            ui.add_space(8.0);
        }

        ui.group(|ui| {
            ui.set_width(ui.available_width());

            ui.heading(panel.title());
            ui.add_space(4.0);

            egui::Grid::new(("declarative_control_panel", panel.id()))
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    for control in panel.controls() {
                        show_control(ui, control);
                    }
                });
        });
    }
}

fn show_control(ui: &mut egui::Ui, control: &ControlState) {
    match control {
        ControlState::Readout { label, text, .. } => {
            ui.label(label);
            ui.strong(text);
            ui.end_row();
        }

        ControlState::Number {
            label,
            value,
            minimum,
            maximum,
            step,
            ..
        } => {
            ui.label(label);

            let mut displayed_value = *value;

            let mut editor = egui::DragValue::new(&mut displayed_value).speed(*step);

            editor = match (*minimum, *maximum) {
                (Some(minimum), Some(maximum)) => editor.range(minimum..=maximum),

                (Some(minimum), None) => editor.range(minimum..=f64::INFINITY),

                (None, Some(maximum)) => editor.range(f64::NEG_INFINITY..=maximum),

                (None, None) => editor,
            };

            ui.add_enabled(false, editor)
                .on_disabled_hover_text("Lua callback is not connected yet");

            ui.end_row();
        }

        ControlState::Toggle { label, value, .. } => {
            ui.label(label);

            let mut displayed_value = *value;

            ui.add_enabled(false, egui::Checkbox::without_text(&mut displayed_value))
                .on_disabled_hover_text("Lua callback is not connected yet");

            ui.end_row();
        }

        ControlState::Button { label, .. } => {
            ui.label("");

            ui.add_enabled(false, egui::Button::new(label))
                .on_disabled_hover_text("Lua callback is not connected yet");

            ui.end_row();
        }
    }
}
