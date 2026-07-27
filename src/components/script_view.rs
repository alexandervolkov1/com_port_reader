use eframe::egui;
use rfd::FileDialog;

use crate::app_log::LogHandle;

use super::{
    command_model::CommandModel, controls_model::ControlsModel,
    device_emulator_model::DeviceEmulatorModel, script_model::ScriptModel,
    serial_settings_model::SerialSettingsModel,
};

pub fn show(
    ui: &mut egui::Ui,
    model: &mut ScriptModel,
    commands: &CommandModel,
    controls: &mut ControlsModel,
    serial_settings: &SerialSettingsModel,
    device_emulator: &mut DeviceEmulatorModel,
    log: &LogHandle,
) {
    if ui.button("Run script...").clicked() {
        let selected_file = FileDialog::new()
            .set_title("Run signal script")
            .set_directory("signal_scripts")
            .add_filter("Signal scripts", &["signals", "txt"])
            .pick_file();

        if let Some(path) = selected_file {
            model.run_file(
                &path,
                commands,
                controls,
                serial_settings,
                device_emulator,
                log,
            );
        }
    }
}
