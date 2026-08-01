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
         recording, series, instruments and the \
         device emulator through the global 'app' \
         table.",
    );

    ui.label(
        "Device model scripts implement the behaviour \
         of a virtual instrument. They communicate \
         with the application only through the \
         configured COM-port pair.",
    );

    ui.separator();

    ui.heading("Lua REPL");

    ui.label(
        "Enter a Lua expression or a multiline Lua \
         statement in the editor.",
    );

    ui.label(
        "Press Ctrl+Enter or click Execute to run \
         the entered code.",
    );

    ui.label(
        "Commands, returned values and Lua errors are \
         displayed in the REPL history.",
    );

    ui.label(
        "Actions affecting the application are also \
         written to the application log.",
    );

    ui.label(
        "Variables and functions remain available \
         between REPL commands and executed scripts.",
    );

    ui.label(
        "Use 'Run script...' to execute a Lua file. \
         The file is evaluated by the same runtime \
         as the REPL.",
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

    ui.heading("Metakon 5X3 controller");

    reference(
        ui,
        "controller = app.metakon()",
        "Creates a Metakon 5X3 controller using the \
         default device and channel.",
    );

    reference(
        ui,
        "controller = app.metakon(options)",
        "Creates a controller with explicitly \
         configured instrument parameters.",
    );

    ui.monospace(
        "controller = app.metakon({\n\
         \x20   device = 15,\n\
         \x20   channel = 0,\n\
         \x20   scale = 1.0,\n\
         })",
    );

    ui.add_space(8.0);

    ui.strong("Metakon defaults:");

    ui.monospace(
        "device = 1\n\
         channel = 0\n\
         scale = 1.0",
    );

    ui.add_space(8.0);

    ui.label(
        "Assigning the controller to a global \
         variable makes it available later from \
         the REPL.",
    );

    ui.label(
        "The Rust driver selects register addresses \
         and data types. Lua code does not need to \
         know the Metakon register map.",
    );

    ui.label(
        "Unknown option names are rejected. This \
         prevents misspelled instrument parameters \
         from silently using defaults.",
    );

    ui.separator();

    ui.heading("Metakon acquisition series");

    reference(
        ui,
        "controller:add_measurement(name)",
        "Adds the measured-value series. The name \
         argument is optional.",
    );

    reference(
        ui,
        "controller:add_setpoint(name)",
        "Adds the current PID setpoint series.",
    );

    reference(
        ui,
        "controller:add_output_power(name)",
        "Adds the current output-power series. Output \
         power is reported from -100 to 100.",
    );

    reference(
        ui,
        "controller:add_proportional_band(name)",
        "Adds the current PID proportional-band \
         series.",
    );

    reference(
        ui,
        "controller:add_integral_time(name)",
        "Adds the current PID integral-time series.",
    );

    reference(
        ui,
        "controller:add_derivative_time(name)",
        "Adds the current PID derivative-time series.",
    );

    ui.monospace(
        "controller:add_measurement(\"temperature\")\n\
         controller:add_setpoint(\"setpoint\")\n\
         controller:add_output_power(\"power\")\n\
         controller:add_proportional_band(\"pid_p\")\n\
         controller:add_integral_time(\"pid_i\")\n\
         controller:add_derivative_time(\"pid_d\")",
    );

    ui.add_space(8.0);

    ui.label(
        "The controller scale is applied to the \
         measurement and setpoint series before \
         values are stored and plotted.",
    );

    ui.separator();

    ui.heading("Metakon PID control");

    reference(
        ui,
        "controller:setpoint(value)",
        "Changes the PID setpoint. The value must be \
         an integer from -999 to 9999.",
    );

    reference(
        ui,
        "controller:proportional_band(value)",
        "Changes the PID proportional band. The value \
         must be an integer from 1 to 9999.",
    );

    reference(
        ui,
        "controller:integral_time(value)",
        "Changes the PID integral time in seconds. \
         The value must be an integer from 1 to \
         30000.",
    );

    reference(
        ui,
        "controller:derivative_time(value)",
        "Changes the PID derivative time in seconds. \
         The value must be an integer from 0 to 255.",
    );

    ui.monospace(
        "controller:setpoint(150)\n\
         controller:proportional_band(250)\n\
         controller:integral_time(120)\n\
         controller:derivative_time(10)",
    );

    ui.add_space(8.0);

    ui.label(
        "The controller scale is not applied when \
         writing PID parameters.",
    );

    ui.label(
        "Write commands are sent through the worker. \
         Periodic acquisition keeps priority over \
         interactive commands.",
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
        "The application COM port and its serial-line \
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

    ui.heading("Metakon application script");

    ui.label(
        "Application scripts are normally stored in \
         the lua_scripts directory and executed with \
         'Run script...'.",
    );

    ui.monospace(
        "app.stop()\n\
         app.clear()\n\n\
         controller = app.metakon({\n\
         \x20   device = 15,\n\
         \x20   channel = 0,\n\
         \x20   scale = 1.0,\n\
         })\n\n\
         controller:add_measurement(\n\
         \x20   \"temperature\"\n\
         )\n\n\
         controller:add_setpoint(\n\
         \x20   \"setpoint\"\n\
         )\n\n\
         controller:add_output_power(\n\
         \x20   \"power\"\n\
         )\n\n\
         app.start()",
    );

    ui.add_space(8.0);

    ui.label(
        "Because controller is global in this example, \
         it can later be used from the REPL:",
    );

    ui.monospace("controller:setpoint(150)");

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
         are interrupted and reported in the REPL.",
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
