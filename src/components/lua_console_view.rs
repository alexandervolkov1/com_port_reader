use eframe::egui;

use crate::components::lua_console_model::LuaConsoleModel;

pub fn show(ui: &mut egui::Ui, model: &mut LuaConsoleModel) {
    ui.horizontal(|ui| {
        ui.label("Lua:");

        if model.is_pending() {
            ui.spinner();
        }

        let enabled = model.is_available();

        let response = ui.add_enabled(
            enabled,
            egui::TextEdit::singleline(model.command_buffer_mut()).desired_width(f32::INFINITY),
        );

        let enter_pressed = enabled
            && response.lost_focus()
            && ui.input(|input| input.key_pressed(egui::Key::Enter));

        if enter_pressed {
            model.submit();
        }

        if model.take_focus_request() {
            response.request_focus();
        }
    });
}
