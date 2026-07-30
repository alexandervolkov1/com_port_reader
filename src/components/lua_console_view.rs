use eframe::egui;
use rfd::FileDialog;

use crate::components::lua_console_model::{LuaConsoleModel, LuaTranscriptEntry};

const MIN_PANEL_WIDTH: f32 = 220.0;
const EDITOR_HEIGHT: f32 = 200.0;

const COMMAND_COLOR: egui::Color32 = egui::Color32::from_rgb(60, 120, 200);

const RESULT_COLOR: egui::Color32 = egui::Color32::from_rgb(60, 150, 90);

const ERROR_COLOR: egui::Color32 = egui::Color32::from_rgb(200, 70, 70);

pub fn show_menu_button(ui: &mut egui::Ui, model: &mut LuaConsoleModel) {
    if ui.selectable_label(model.is_open(), "Lua REPL").clicked() {
        model.toggle_open();
    }
}

pub fn show_panel(root_ui: &mut egui::Ui, model: &mut LuaConsoleModel) {
    if !model.is_open() {
        return;
    }

    let default_panel_width = (root_ui.available_width() / 3.0).max(MIN_PANEL_WIDTH);

    egui::Panel::left("lua_repl_panel")
        .resizable(true)
        .default_size(default_panel_width)
        .min_size(MIN_PANEL_WIDTH)
        .show(root_ui, |ui| {
            show_toolbar(ui, model);

            ui.separator();

            show_editor(ui, model);

            ui.separator();

            show_transcript(ui, model);
        });
}

fn show_toolbar(ui: &mut egui::Ui, model: &mut LuaConsoleModel) {
    ui.horizontal(|ui| {
        let available = model.is_available();

        if ui
            .add_enabled(available, egui::Button::new("Run script..."))
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
            .add_enabled(available, egui::Button::new("Execute"))
            .clicked()
        {
            model.submit();
        }

        if ui.button("Clear history").clicked() {
            model.clear_transcript();
        }

        if model.is_pending() {
            ui.spinner();
        }
    });
}

fn show_editor(ui: &mut egui::Ui, model: &mut LuaConsoleModel) {
    let available = model.is_available();

    ui.horizontal_top(|ui| {
        ui.colored_label(COMMAND_COLOR, ">");

        let editor_width = ui.available_width();

        let response = ui.add_enabled(
            available,
            egui::TextEdit::multiline(model.command_buffer_mut())
                .code_editor()
                .desired_width(editor_width)
                .desired_rows(8)
                .min_size(egui::vec2(editor_width, EDITOR_HEIGHT)),
        );

        let execute = available
            && response.has_focus()
            && ui.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::Enter));

        if execute {
            model.submit();
        }

        if model.take_focus_request() {
            response.request_focus();
        }
    });

    ui.small("Ctrl+Enter to execute");
}

fn show_transcript(ui: &mut egui::Ui, model: &LuaConsoleModel) {
    egui::ScrollArea::vertical()
        .id_salt("lua_repl_history")
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            for entry in model.transcript() {
                show_transcript_entry(ui, entry);

                ui.add_space(4.0);
            }
        });
}

fn show_transcript_entry(ui: &mut egui::Ui, entry: &LuaTranscriptEntry) {
    match entry {
        LuaTranscriptEntry::Command(command) => {
            ui.horizontal_top(|ui| {
                ui.colored_label(COMMAND_COLOR, ">");

                ui.add(
                    egui::Label::new(
                        egui::RichText::new(command)
                            .monospace()
                            .color(COMMAND_COLOR),
                    )
                    .wrap(),
                );
            });
        }

        LuaTranscriptEntry::Result(result) => {
            ui.add(
                egui::Label::new(egui::RichText::new(result).monospace().color(RESULT_COLOR))
                    .wrap(),
            );
        }

        LuaTranscriptEntry::Error(error) => {
            ui.add(
                egui::Label::new(egui::RichText::new(error).monospace().color(ERROR_COLOR)).wrap(),
            );
        }
    }
}
