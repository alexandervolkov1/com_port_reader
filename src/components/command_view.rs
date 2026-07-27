use eframe::egui;

use crate::components::{
    command_model::CommandModel, controls_model::ControlsModel,
    device_emulator_model::DeviceEmulatorModel, serial_settings_model::SerialSettingsModel,
};

pub fn show(
    ui: &mut egui::Ui,
    command: &mut CommandModel,
    controls: &mut ControlsModel,
    serial_settings: &SerialSettingsModel,
    device_emulator: &mut DeviceEmulatorModel,
) {
    ui.horizontal(|ui| {
        ui.label("Command:");

        let response = ui.text_edit_singleline(command.command_buffer_mut());

        if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
            command.submit(controls, serial_settings, device_emulator);
            response.request_focus();
        }
    });
}
