use eframe::egui;
use rfd::FileDialog;

use crate::components::lua_console_model::{LuaConsoleModel, LuaTranscriptEntry};

const MIN_PANEL_WIDTH: f32 = 220.0;
const EDITOR_ROWS: usize = 4;

pub fn show_menu_button(ui: &mut egui::Ui, model: &mut LuaConsoleModel) {
    if ui.selectable_label(model.is_open(), "Lua REPL").clicked() {
        model.toggle_open();
    }
}

pub fn show_panel(root_ui: &mut egui::Ui, model: &mut LuaConsoleModel) {
    if !model.is_open() {
        return;
    }

    let close_requested = false;

    let default_panel_width = (root_ui.available_width() / 3.0).max(MIN_PANEL_WIDTH);

    egui::Panel::left("lua_repl_panel")
        .resizable(true)
        .default_size(default_panel_width)
        .min_size(MIN_PANEL_WIDTH)
        .show(root_ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Lua REPL");
            });

            show_toolbar(ui, model);

            ui.separator();

            let response = show_terminal(ui, model);

            let execute_shortcut = model.is_available()
                && response.has_focus()
                && ui.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::Enter));

            if execute_shortcut {
                model.submit();
            }

            if model.take_focus_request() {
                response.request_focus();
            }
        });

    if close_requested {
        model.close();
    }
}

fn show_toolbar(ui: &mut egui::Ui, model: &mut LuaConsoleModel) {
    ui.horizontal(|ui| {
        if ui
            .add_enabled(model.is_available(), egui::Button::new("Run script..."))
            .clicked()
            && let Some(path) = FileDialog::new()
                .set_title("Run Lua script")
                .set_directory("lua_scripts")
                .add_filter("Lua scripts", &["lua"])
                .pick_file()
        {
            model.run_file(&path);
        }

        if ui
            .add_enabled(model.can_submit(), egui::Button::new("Execute"))
            .on_hover_text(
                "Execute Lua code \
                 (Ctrl+Enter)",
            )
            .clicked()
        {
            model.submit();
        }

        if ui
            .add_enabled(model.has_transcript(), egui::Button::new("Clear"))
            .on_hover_text("Clear REPL output")
            .clicked()
        {
            model.clear_transcript();
        }

        if model.is_pending() {
            ui.spinner();
        }
    });
}

fn show_terminal(ui: &mut egui::Ui, model: &mut LuaConsoleModel) -> egui::Response {
    egui::ScrollArea::vertical()
        .id_salt("lua_repl_terminal")
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for entry in model.transcript() {
                show_entry(ui, entry);

                ui.add_space(6.0);
            }

            show_prompt(ui, model)
        })
        .inner
}

fn show_prompt(ui: &mut egui::Ui, model: &mut LuaConsoleModel) -> egui::Response {
    let enabled = model.is_available();

    let first_prompt = !model.has_transcript();

    ui.horizontal_top(|ui| {
        ui.label(
            egui::RichText::new(">")
                .monospace()
                .color(egui::Color32::from_rgb(60, 120, 200)),
        );

        let available_width = ui.available_width();

        let editor = egui::TextEdit::multiline(model.command_buffer_mut())
            .code_editor()
            .desired_rows(EDITOR_ROWS)
            .desired_width(available_width)
            .hint_text(if first_prompt { "Enter Lua code" } else { "" });

        ui.add_enabled(enabled, editor)
    })
    .inner
}

fn show_entry(ui: &mut egui::Ui, entry: &LuaTranscriptEntry) {
    match entry {
        LuaTranscriptEntry::Command(source) => {
            let source = source.replace('\n', "\n  ");

            ui.label(
                egui::RichText::new(format!("> {source}",))
                    .monospace()
                    .color(egui::Color32::from_rgb(60, 120, 200)),
            );
        }

        LuaTranscriptEntry::Result(output) => {
            ui.label(
                egui::RichText::new(output)
                    .monospace()
                    .color(egui::Color32::from_rgb(60, 150, 90)),
            );
        }

        LuaTranscriptEntry::Error(error) => {
            ui.label(
                egui::RichText::new(error)
                    .monospace()
                    .color(egui::Color32::from_rgb(60, 150, 90)),
            );
        }
    }
}
