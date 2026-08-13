use eframe::egui;

use super::control_panel_model::{ControlPanelModel, ControlState};

use crate::lua_application_script::{LuaControlArgument, LuaControlInvocation};

const VIEWPORT_ID: &str = "application_control_panel";

pub fn show_menu_button(ui: &mut egui::Ui, model: &ControlPanelModel, open: &mut bool) {
    let configured = !model.panels().is_empty();

    let response = ui.add_enabled(configured, egui::Button::new("Control panel"));

    if response.clicked() {
        *open = !*open;
    }

    if !configured {
        response.on_disabled_hover_text("No control panels are defined by application scripts");
    }
}

pub(crate) fn show_viewport(
    root_ui: &mut egui::Ui,
    model: &mut ControlPanelModel,
    open: &mut bool,
) -> Vec<LuaControlInvocation> {
    let mut invocations = Vec::new();

    if !*open {
        return invocations;
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
                        show_contents(ui, model, &mut invocations);
                    });
            });
        },
    );

    invocations
}

fn show_contents(
    ui: &mut egui::Ui,
    model: &mut ControlPanelModel,
    invocations: &mut Vec<LuaControlInvocation>,
) {
    for (panel_index, panel) in model.panels_mut().iter_mut().enumerate() {
        if panel_index > 0 {
            ui.add_space(8.0);
        }

        let script_id = panel.script_id().to_owned();
        let panel_id = panel.id().to_owned();
        let title = panel.title().to_owned();

        ui.group(|ui| {
            ui.set_width(ui.available_width());

            ui.heading(title);
            ui.add_space(4.0);

            egui::Grid::new((
                "declarative_control_panel",
                script_id.as_str(),
                panel_id.as_str(),
            ))
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                for control in panel.controls_mut() {
                    show_control(ui, &script_id, &panel_id, control, invocations);
                }
            });
        });
    }
}

fn show_control(
    ui: &mut egui::Ui,
    script_id: &str,
    panel_id: &str,
    control: &mut ControlState,
    invocations: &mut Vec<LuaControlInvocation>,
) {
    match control {
        ControlState::Readout { label, text, .. } => {
            ui.label(label.as_str());
            ui.strong(text.as_str());
            ui.end_row();
        }

        ControlState::Number {
            id,
            label,
            draft_value,
            minimum,
            maximum,
            step,
            on_change,
            ..
        } => {
            ui.label(label.as_str());

            let mut editor = egui::DragValue::new(draft_value)
                .speed(*step)
                .update_while_editing(false);

            editor = match (*minimum, *maximum) {
                (Some(minimum), Some(maximum)) => editor.range(minimum..=maximum),

                (Some(minimum), None) => editor.range(minimum..=f64::INFINITY),

                (None, Some(maximum)) => editor.range(f64::NEG_INFINITY..=maximum),

                (None, None) => editor,
            };

            let response = ui.add(editor);

            let submitted = response.drag_stopped() || (response.changed() && !response.dragged());

            if submitted {
                invocations.push(LuaControlInvocation::new(
                    script_id,
                    panel_id,
                    id.as_str(),
                    on_change.as_str(),
                    Some(LuaControlArgument::Number(*draft_value)),
                ));
            }

            ui.end_row();
        }

        ControlState::Toggle {
            id,
            label,
            draft_value,
            on_change,
            ..
        } => {
            ui.label(label.as_str());

            if ui.add(egui::Checkbox::without_text(draft_value)).changed() {
                invocations.push(LuaControlInvocation::new(
                    script_id,
                    panel_id,
                    id.as_str(),
                    on_change.as_str(),
                    Some(LuaControlArgument::Boolean(*draft_value)),
                ));
            }

            ui.end_row();
        }

        ControlState::Button {
            id,
            label,
            on_click,
        } => {
            ui.label("");

            if ui.button(label.as_str()).clicked() {
                invocations.push(LuaControlInvocation::new(
                    script_id,
                    panel_id,
                    id.as_str(),
                    on_click.as_str(),
                    None,
                ));
            }

            ui.end_row();
        }
    }
}
