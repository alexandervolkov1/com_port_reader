use crate::{components::command_model::CommandModel, data::Signal};
use eframe::egui;

pub fn show(ui: &mut egui::Ui, command: &mut CommandModel) {
    ui.horizontal(|ui| {
        ui.label("Command:");

        let response = ui.text_edit_singleline(&mut command.command_buffer);

        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            match Signal::from_string(&command.command_buffer) {
                Ok(signal) => {
                    let _ = command.command_sender.send(signal);
                }
                Err(e) => {
                    command.last_response = e;
                }
            }

            command.command_buffer.clear();
            response.request_focus();
        }
    });

    ui.label(&command.last_response);
}
