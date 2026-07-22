use eframe::egui::{self, DragValue};

use crate::{
    components::{
        command_model::CommandModel,
        series_editor_model::{SeriesDraft, SeriesEditorModel},
    },
    data::SignalKind,
    user_command::UserCommand,
};

pub fn show(context: &egui::Context, editor: &mut SeriesEditorModel, commands: &mut CommandModel) {
    if !editor.is_open() {
        return;
    }

    let mut window_open = true;
    let mut add_requested = false;
    let mut cancel_requested = false;

    egui::Window::new("Add new series")
        .collapsible(false)
        .resizable(false)
        .open(&mut window_open)
        .show(context, |ui| {
            render_form(ui, editor.draft_mut());

            if let Some(error) = editor.error() {
                ui.colored_label(egui::Color32::RED, error);
            }

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Add").clicked() {
                    add_requested = true;
                }

                if ui.button("Cancel").clicked() {
                    cancel_requested = true;
                }
            });
        });

    if !window_open || cancel_requested {
        editor.close();
        return;
    }

    if add_requested {
        match editor.build() {
            Ok(new_series) => {
                commands.execute(UserCommand::Add(new_series));
                editor.close();
            }

            Err(error) => {
                editor.set_error(error.to_string());
            }
        }
    }
}

fn render_form(ui: &mut egui::Ui, draft: &mut SeriesDraft) {
    ui.horizontal(|ui| {
        ui.label("Name:");
        ui.text_edit_singleline(&mut draft.name);
    });

    egui::ComboBox::from_label("Signal")
        .selected_text(draft.kind.name())
        .show_ui(ui, |ui| {
            for kind in SignalKind::ALL {
                ui.selectable_value(&mut draft.kind, kind, kind.name());
            }
        });

    match draft.kind {
        SignalKind::Sine => {
            number_field(ui, "Amplitude:", &mut draft.amplitude, 1.0);
            number_field(ui, "Period:", &mut draft.period, 1.0);
            number_field(ui, "Phase:", &mut draft.phase, 0.01);
        }

        SignalKind::Square => {
            number_field(ui, "Amplitude:", &mut draft.amplitude, 1.0);
            number_field(ui, "Period:", &mut draft.period, 1.0);
            number_field(ui, "Duty cycle:", &mut draft.duty_cycle, 0.01);
        }

        SignalKind::Triangle | SignalKind::Sawtooth => {
            number_field(ui, "Amplitude:", &mut draft.amplitude, 1.0);
            number_field(ui, "Period:", &mut draft.period, 1.0);
        }

        SignalKind::Constant => {
            number_field(ui, "Value:", &mut draft.value, 1.0);
        }
    }
}

fn number_field(ui: &mut egui::Ui, label: &str, value: &mut f64, speed: f64) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(DragValue::new(value).speed(speed));
    });
}
