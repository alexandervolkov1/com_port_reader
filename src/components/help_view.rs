use eframe::egui;

use super::help_model::HelpModel;

pub fn show_menu_button(ui: &mut egui::Ui, model: &mut HelpModel) {
    ui.menu_button("Help", |ui| {
        if ui.button("Lua reference").clicked() {
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

    egui::Window::new("Lua reference")
        .open(&mut open)
        .default_size(egui::vec2(760.0, 650.0))
        .resizable(true)
        .show(context, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, show_lua_reference);
        });

    model.set_command_reference_open(open);
}

fn show_lua_reference(ui: &mut egui::Ui) {
    ui.heading("Lua environments");

    ui.label(
        "The application uses two independent Lua \
         environments.",
    );

    ui.label(
        "Application scripts control acquisition, \
         recording, series and the device emulator \
         through the global 'app' table.",
    );

    ui.label(
        "Device model scripts implement the behaviour \
         of a virtual instrument. They communicate \
         with the application only through the \
         configured COM-port pair.",
    );

    ui.separator();

    ui.heading("Lua console");

    ui.label(
        "Enter a Lua expression or statement in the \
         Lua field and press Enter.",
    );

    ui.label(
        "Expression results and errors are written \
         to the application log.",
    );

    ui.label(
        "Variables and functions remain available \
         between console commands and executed \
         application scripts.",
    );

    ui.label(
        "Use 'Run Lua...' to execute a Lua file. \
         The file is evaluated by the same runtime \
         as the console.",
    );

    ui.separator();

    ui.heading("Acquisition");

    reference(
        ui,
        "app.start()",
        "Starts periodic acquisition using the poll \
         interval configured in Settings.",
    );

    reference(
        ui,
        "app.stop()",
        "Stops periodic acquisition. Active CSV \
         recording remains open and paused.",
    );

    reference(
        ui,
        "app.clear()",
        "Removes all series and their accumulated \
         samples.",
    );

    ui.separator();

    ui.heading("Serial series");

    reference(
        ui,
        "app.add_serial(command)",
        "Adds a periodically sampled serial series. \
         A unique name is generated automatically.",
    );

    reference(
        ui,
        "app.add_serial(command, name)",
        "Adds a periodically sampled serial series \
         with an explicit name.",
    );

    ui.monospace(
        "app.add_serial(\n\
         \x20   \"read temperature\",\n\
         \x20   \"temperature\"\n\
         )",
    );

    ui.add_space(8.0);

    ui.label(
        "The command must produce a response that can \
         be parsed as one finite f64 value.",
    );

    ui.label(
        "Series names must be unique and cannot \
         contain whitespace.",
    );

    ui.separator();

    ui.heading("Metakon series");

    reference(
        ui,
        "app.add_metakon()",
        "Adds a Metakon series using all default \
         parameters.",
    );

    reference(
        ui,
        "app.add_metakon(options)",
        "Adds a periodically sampled Metakon register. \
         Parameters are passed in a Lua table.",
    );

    ui.monospace(
        "app.add_metakon({\n\
         \x20   device = 15,\n\
         \x20   channel = 0,\n\
         \x20   register = 0x01,\n\
         \x20   scale = 0.1,\n\
         \x20   name = \"temperature\",\n\
         })",
    );

    ui.add_space(8.0);

    ui.strong("Metakon defaults:");

    ui.monospace(
        "device = 1\n\
         channel = 0\n\
         register = 0x01\n\
         scale = 1.0\n\
         name = automatic",
    );

    ui.add_space(8.0);

    ui.label(
        "The raw signed register value is multiplied \
         by scale before being stored and plotted.",
    );

    ui.label(
        "Unknown option names are rejected. This \
         prevents misspelled instrument parameters \
         from silently using defaults.",
    );

    ui.separator();

    ui.heading("Series management");

    reference(
        ui,
        "app.delete(name)",
        "Deletes the named series and all its \
         accumulated samples.",
    );

    reference(
        ui,
        "app.rename(current_name, new_name)",
        "Renames an existing series without changing \
         its samples.",
    );

    ui.monospace(
        "app.rename(\n\
         \x20   \"temperature\",\n\
         \x20   \"furnace_temperature\"\n\
         )\n\n\
         app.delete(\"furnace_temperature\")",
    );

    ui.separator();

    ui.heading("Serial commands");

    reference(
        ui,
        "app.send_serial(command)",
        "Sends one text command to the application \
         COM port and writes the response to the log.",
    );

    ui.monospace(
        "app.send_serial(\n\
         \x20   \"set amplitude 25\"\n\
         )",
    );

    ui.add_space(8.0);

    ui.label(
        "The application COM port and its serial line \
         settings are selected in Settings.",
    );

    ui.separator();

    ui.heading("CSV recording");

    reference(
        ui,
        "app.start_rec()",
        "Creates a new protocol file and starts CSV \
         recording. If acquisition is stopped, \
         recording remains paused until acquisition \
         starts.",
    );

    reference(
        ui,
        "app.stop_rec()",
        "Flushes and closes the active protocol file.",
    );

    ui.label(
        "Protocol files are grouped by date inside \
         the protocols directory.",
    );

    ui.separator();

    ui.heading("Device emulator");

    reference(
        ui,
        "app.start_emu()",
        "Starts the device emulator using the port \
         and Lua model selected in Settings.",
    );

    reference(ui, "app.stop_emu()", "Stops the running device emulator.");

    ui.label(
        "Calling app.start_emu() while the emulator \
         is already running does not reload its \
         model.",
    );

    ui.label(
        "To reload an edited device model and reset \
         its state, stop and start the emulator.",
    );

    ui.monospace(
        "app.stop_emu()\n\
         app.start_emu()",
    );

    ui.separator();

    ui.heading("Application script example");

    ui.label(
        "Application scripts are normally stored in \
         the lua_scripts directory and executed with \
         'Run Lua...'.",
    );

    ui.monospace(
        "app.stop()\n\
         app.stop_emu()\n\
         app.clear()\n\n\
         app.start_emu()\n\n\
         app.add_serial(\n\
         \x20   \"read phase_a\",\n\
         \x20   \"phase_A\"\n\
         )\n\n\
         app.add_serial(\n\
         \x20   \"read phase_b\",\n\
         \x20   \"phase_B\"\n\
         )\n\n\
         app.add_serial(\n\
         \x20   \"read phase_c\",\n\
         \x20   \"phase_C\"\n\
         )\n\n\
         app.start()",
    );

    ui.separator();

    ui.heading("Device model scripts");

    ui.label(
        "Device models are normally stored in the \
         emulator_scripts directory and selected \
         in Settings.",
    );

    ui.label(
        "A device model must define the global \
         function:",
    );

    reference(
        ui,
        "handle(command, time)",
        "Receives one command and elapsed time in \
         seconds since the emulator started. It must \
         return one response string.",
    );

    ui.monospace(
        "local value = 42.0\n\n\
         function handle(command, time)\n\
         \x20   if command == \"read value\" then\n\
         \x20       return tostring(value)\n\
         \x20   end\n\n\
         \x20   return \"error unknown command: \"\n\
         \x20       .. command\n\
         end",
    );

    ui.add_space(8.0);

    ui.label(
        "The emulator reloads the selected model \
         only when it starts. Script state is \
         preserved between commands while the \
         emulator remains running.",
    );

    ui.separator();

    ui.heading("Execution limits");

    ui.label(
        "One Lua execution is limited to 500 ms. \
         Infinite loops and blocking process logic \
         are interrupted and reported in the log.",
    );

    ui.label(
        "Lua scripts should configure the application \
         and return promptly. Long technological \
         procedures will use a non-blocking event \
         model in a later stage.",
    );
}

fn reference(ui: &mut egui::Ui, syntax: &str, description: &str) {
    ui.monospace(syntax);
    ui.label(description);
    ui.add_space(8.0);
}
