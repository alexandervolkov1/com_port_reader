mod english;
mod examples;
mod russian;

use eframe::egui;

use super::help_model::{HelpLanguage, HelpModel};

pub fn show_menu_button(ui: &mut egui::Ui, model: &mut HelpModel) {
    ui.menu_button("Help", |ui| {
        if ui.button("Lua reference / Справка Lua").clicked() {
            model.open_command_reference();
            ui.close();
        }
    });
}

pub fn show_window(context: &egui::Context, model: &mut HelpModel) {
    let mut open = model.command_reference_open();

    if !open {
        return;
    }

    let mut language = model.language();

    egui::Window::new("Lua reference / Справка Lua")
        .open(&mut open)
        .default_size(egui::vec2(780.0, 680.0))
        .resizable(true)
        .show(context, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut language, HelpLanguage::English, "English");

                ui.selectable_value(&mut language, HelpLanguage::Russian, "Русский");
            });

            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    match language {
                        HelpLanguage::English => {
                            english::show(ui);
                        }

                        HelpLanguage::Russian => {
                            russian::show(ui);
                        }
                    }
                });
        });

    model.set_language(language);
    model.set_command_reference_open(open);
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.separator();
    ui.heading(title);
}

fn reference(ui: &mut egui::Ui, syntax: &str, description: &str) {
    ui.monospace(syntax);
    ui.label(description);
    ui.add_space(6.0);
}

fn code(ui: &mut egui::Ui, source: &str) {
    ui.add_space(4.0);
    ui.monospace(source);
    ui.add_space(8.0);
}
