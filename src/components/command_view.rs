use eframe::egui;

use crate::components::command_model::CommandModel;

pub fn show(ui: &mut egui::Ui, command: &mut CommandModel) {
    ui.horizontal(|ui| {
        ui.label("Command:");

        let response = ui.text_edit_singleline(command.command_buffer_mut());

        if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
            command.submit();
            response.request_focus();
        }
    });
}
