use eframe::egui;
use rfd::FileDialog;

use crate::components::lua_console_model::LuaConsoleModel;

const MAX_CONSOLE_WIDTH: f32 = 600.0;

pub fn show(ui: &mut egui::Ui, model: &mut LuaConsoleModel) {
    ui.horizontal(|ui| {
        ui.label("Lua:");

        if model.is_pending() {
            ui.spinner();
        }

        let enabled = model.is_available();

        if ui
            .add_enabled(enabled, egui::Button::new("Run Lua..."))
            .clicked()
            && let Some(path) = FileDialog::new()
                .set_title("Run Lua script")
                .set_directory("lua_scripts")
                .add_filter("Lua scripts", &["lua"])
                .pick_file()
        {
            model.run_file(&path);
        }

        let editor_width = ui.available_width().min(MAX_CONSOLE_WIDTH).max(120.0);

        let response = ui.add_enabled(
            enabled,
            egui::TextEdit::singleline(model.command_buffer_mut()).desired_width(editor_width),
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
