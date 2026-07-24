use eframe::egui;

use super::help_model::HelpModel;

pub fn show_menu(ui: &mut egui::Ui, model: &mut HelpModel) {
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("Help", |ui| {
            if ui.button("Command reference").clicked() {
                model.open_command_reference();
                ui.close();
            }
        });
    });
}

pub fn show_window(context: &egui::Context, model: &mut HelpModel) {
    let mut open = model.command_reference_open();

    if !open {
        return;
    }

    egui::Window::new("Command reference")
        .open(&mut open)
        .default_size(egui::vec2(720.0, 600.0))
        .resizable(true)
        .show(context, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, show_command_reference);
        });

    model.set_command_reference_open(open);
}

fn show_command_reference(ui: &mut egui::Ui) {
    ui.heading("General syntax");

    ui.label(
        "The 'add' keyword is optional. Parameters may \
         be positional or written as named options.",
    );

    reference(ui, "add <series definition>", "Adds a new signal series.");

    reference(
        ui,
        "<series definition>",
        "Also adds a series. The leading 'add' may be omitted.",
    );

    ui.label(
        "Series names are optional. If --name is omitted, \
         a unique name is generated automatically.",
    );

    ui.label("Names must be unique and cannot contain whitespace.");

    ui.separator();
    ui.heading("Generated signals");

    reference(
        ui,
        "sin [amplitude] [period] [phase] [--name NAME]",
        "Sine wave. Defaults: amplitude 100, period 100 s, \
         phase 0 rad.",
    );

    reference(
        ui,
        "sin --amp 100 --per 300 --phase 0 --name phase_A",
        "The same command using named parameters.",
    );

    reference(
        ui,
        "square [amplitude] [period] [duty] [--name NAME]",
        "Square wave. Defaults: amplitude 100, period 100 s, \
         duty cycle 0.5.",
    );

    reference(
        ui,
        "triangle [amplitude] [period] [--name NAME]",
        "Triangle wave. Defaults: amplitude 100, \
         period 100 s.",
    );

    reference(
        ui,
        "saw [amplitude] [period] [--name NAME]",
        "Sawtooth wave. Defaults: amplitude 100, \
         period 100 s.",
    );

    reference(
        ui,
        "const [value] [--name NAME]",
        "Constant value. Default: 50.",
    );

    ui.strong("Signal aliases:");

    ui.monospace(
        "sin | sine\n\
         square | sq\n\
         triangle | tri\n\
         saw | sawtooth\n\
         const | constant",
    );

    ui.add_space(8.0);

    ui.strong("Parameter aliases:");

    ui.monospace(
        "--amp | --amplitude\n\
         --per | --period\n\
         --duty | --duty-cycle\n\
         --value | --val",
    );

    ui.separator();
    ui.heading("COM series");

    reference(
        ui,
        "com <command> [--step VALUE] [--name NAME]",
        "Adds a series whose values are read from the selected \
         COM port.",
    );

    reference(
        ui,
        "com walk --step 1 --name random_walk",
        "Reads an independent random walk from the bundled \
         emulator. Default step: 1.",
    );

    reference(
        ui,
        "serial walk",
        "'serial' is an alias for 'com'. The name and step \
         may both be omitted.",
    );

    ui.separator();
    ui.heading("Series management");

    reference(
        ui,
        "delete <name>",
        "Deletes the series with the specified name.",
    );

    reference(
        ui,
        "rename <current_name> <new_name>",
        "Renames an existing series without changing its data.",
    );

    ui.separator();
    ui.heading("Script files");

    ui.label(
        "A script contains one DSL command per line. Empty \
         lines and lines beginning with '#' are ignored.",
    );

    ui.monospace(
        "# Three-phase demonstration\n\
         sin --amp 100 --per 300 --phase 0 --name phase_A\n\
         sin --amp 100 --per 300 --phase 2.094395 --name phase_B\n\
         sin --amp 100 --per 300 --phase 4.188790 --name phase_C",
    );

    ui.add_space(8.0);

    ui.label(
        "Script files can be opened with Start from file. \
         All lines are validated before commands are submitted.",
    );
}

fn reference(ui: &mut egui::Ui, syntax: &str, description: &str) {
    ui.monospace(syntax);
    ui.label(description);
    ui.add_space(8.0);
}
