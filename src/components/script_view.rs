use eframe::egui;
use rfd::FileDialog;

use super::{
    command_model::CommandModel, controls_model::ControlsModel, script_model::ScriptModel,
};

pub fn show(
    ui: &mut egui::Ui,
    model: &mut ScriptModel,
    commands: &mut CommandModel,
    controls: &ControlsModel,
) {
    if ui.button("Start from file...").clicked() {
        let selected_file = FileDialog::new()
            .set_title("Select signal script")
            .set_directory("signal_scripts")
            .add_filter("Signal scripts", &["signals", "txt"])
            .pick_file();

        if let Some(path) = selected_file {
            model.start_from_file(&path, commands, controls);
        }
    }

    if let Some(message) = model.message() {
        if model.has_error() {
            ui.colored_label(egui::Color32::RED, message);
        } else {
            ui.colored_label(egui::Color32::DARK_GREEN, message);
        }
    }
}
