use eframe::egui;
use rfd::FileDialog;

use crate::app_log::LogHandle;

use super::{
    command_model::CommandModel, controls_model::ControlsModel, script_model::ScriptModel,
};

pub fn show(
    ui: &mut egui::Ui,
    model: &mut ScriptModel,
    commands: &mut CommandModel,
    controls: &ControlsModel,
    log: &LogHandle,
) {
    if ui.button("Start from file...").clicked() {
        let selected_file = FileDialog::new()
            .set_title("Select signal script")
            .set_directory("signal_scripts")
            .add_filter("Signal scripts", &["signals", "txt"])
            .pick_file();

        if let Some(path) = selected_file {
            model.start_from_file(&path, commands, controls, log);
        }
    }
}
