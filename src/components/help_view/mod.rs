//! Compact function lookup; long tutorials and algorithm details remain in docs/.
mod catalog;

use super::help_model::{HelpCategory, HelpLanguage, HelpModel};
use eframe::egui;
use std::path::Path;

pub fn show_menu_button(ui: &mut egui::Ui, model: &mut HelpModel) {
    ui.menu_button("Help", |ui| {
        if ui.button("Lua reference").clicked() {
            model.open_command_reference();
            ui.close();
        }
    });
}

/// Renders the searchable bilingual catalog without evaluating any displayed example.
/// Copying a snippet only changes the clipboard; opening a guide uses local packaged docs.
pub fn show_window(context: &egui::Context, model: &mut HelpModel, docs_directory: &Path) {
    let mut open = model.command_reference_open();
    if !open {
        return;
    }
    let mut language = model.language();
    egui::Window::new("Lua reference")
        .open(&mut open)
        .default_size(egui::vec2(840.0, 680.0))
        .min_width(440.0)
        .resizable(true)
        .show(context, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut language, HelpLanguage::English, "English");
                ui.selectable_value(&mut language, HelpLanguage::Russian, "Русский");
                ui.separator();
                for (file, en, ru) in [
                    ("lua-api.md", "Full guide", "Полный справочник"),
                    ("lua-tutorial.md", "Tutorial", "Учебник"),
                ] {
                    if ui.link(language.choose(en, ru)).clicked() {
                        model.document_error = open::that(docs_directory.join(file))
                            .err().map(|error| error.to_string());
                    }
                }
            });
            if let Some(error) = &model.document_error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.small(language.choose(
                "Application Lua API, not the Lua standard library. Expand a function for arguments and an example.",
                "API приложения, не стандартная библиотека Lua. Раскройте функцию: внутри аргументы и пример.",
            ));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut model.search)
                    .hint_text(language.choose("Search: function, parameter, purpose…", "Поиск: функция, параметр, назначение…"))
                    .desired_width(ui.available_width() - 70.0));
                if ui.button(language.choose("Clear", "Сброс")).clicked() {
                    model.search.clear();
                    model.category = HelpCategory::All;
                }
            });
            ui.horizontal_wrapped(|ui| {
                for category in HelpCategory::ALL {
                    ui.selectable_value(&mut model.category, category, category.label(language));
                }
            });
            ui.small(language.choose(
                "Examples assume existing plant / loop / scenario handles; controller constructors are alternatives. Copy does not execute.",
                "Примеры предполагают готовые plant / loop / scenario; конструкторы регуляторов — альтернативы. Копирование не запускает код.",
            ));
            ui.separator();
            let entries = catalog::ENTRIES.iter()
                .filter(|entry| entry.matches(model.category, &model.search)).collect::<Vec<_>>();
            ui.label(format!("{}: {}", language.choose("Found", "Найдено"), entries.len()));
            egui::ScrollArea::vertical()
                .id_salt(("lua_reference_results", model.category, &model.search))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if entries.is_empty() {
                        ui.label(language.choose(
                            "No matches. Shorten the query or choose All.",
                            "Совпадений нет. Сократите запрос или выберите «Все».",
                        ));
                    }
                    for entry in entries {
                        egui::CollapsingHeader::new(egui::RichText::new(entry.signature).monospace())
                            .id_salt(entry.signature)
                            .show(ui, |ui| {
                                ui.label(entry.details(language));
                                if ui.small_button(language.choose("Copy example", "Копировать пример")).clicked() {
                                    ui.ctx().copy_text(entry.example.to_owned());
                                }
                                ui.add(egui::Label::new(egui::RichText::new(entry.example).monospace()).wrap());
                            });
                        ui.small(entry.summary(language));
                        ui.add_space(5.0);
                    }
                });
        });
    model.set_language(language);
    model.set_command_reference_open(open);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_renders_both_languages_and_empty_search_results() {
        let context = egui::Context::default();
        let mut model = HelpModel::default();
        model.open_command_reference();
        for language in [HelpLanguage::English, HelpLanguage::Russian] {
            model.set_language(language);
            for query in ["", "furnace", "nothing_matches_this"] {
                model.search = query.to_owned();
                let output = context.run_ui(egui::RawInput::default(), |ui| {
                    show_window(ui.ctx(), &mut model, Path::new("docs"));
                });
                assert!(!output.shapes.is_empty());
                assert!(model.command_reference_open());
            }
        }
    }
}
