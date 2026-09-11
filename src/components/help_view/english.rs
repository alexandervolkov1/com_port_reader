use eframe::egui;

use super::{code, examples::*, reference, section};

pub(super) fn show(ui: &mut egui::Ui) {
    ui.heading("Lua environments");

    ui.label(
        "The application uses two independent Lua \
         environments.",
    );

    ui.label(
        "Application scripts and the REPL control \
         acquisition, series, instruments and the \
         device emulator through the global 'app' \
         table.",
    );

    ui.label(
        "Device-model scripts run inside the emulator \
         and describe virtual instruments. They do \
         not have access to the application 'app' \
         table.",
    );

    section(ui, "Application configuration");

    ui.label(
        "The application loads the selected startup \
         profile. If no profile was selected, it uses \
         startup.lua from the application directory. \
         The script must return one table.",
    );

    ui.label(
        "Supported root sections are application, \
         connections, emulator, plot_panes, scripts and setup.",
    );

    ui.label(
        "Keep the top level of startup.lua free of \
         side effects. It is evaluated once for \
         validation and once by the application Lua \
         runtime. Put application actions inside \
         setup().",
    );

    code(ui, STARTUP_EXAMPLE);

    ui.label(
        "Relative script and emulator-model paths are \
         resolved from the directory containing the \
         selected startup profile. Logs, process \
         databases and other application data are stored \
         relative to the application directory.",
    );

    ui.label(
        "Use Settings to select and validate another Lua \
         profile before loading it. The active profile is \
         remembered for the next launch. The --config \
         command-line option selects a profile explicitly.",
    );

    ui.label(
        "Loading or reloading a profile replaces the whole \
         runtime: acquisition and the emulator are stopped, \
         registered control panels are removed, and all \
         series and plot history are cleared.",
    );

    section(ui, "Runtime options");

    reference(
        ui,
        "fps",
        "GUI repaint rate from 1 to 240 frames per \
         second. Default: 30.",
    );

    reference(
        ui,
        "poll_interval",
        "Default polling interval in seconds. \
         Default: 1.0.",
    );

    reference(
        ui,
        "plot_window",
        "Live plot window in seconds. Default: \
         3600.0.",
    );

    reference(
        ui,
        "max_plot_points_per_series",
        "Maximum number of prepared points for one \
         visible series. Default: 4000.",
    );

    section(ui, "Declarative plot layout");

    reference(
        ui,
        "plot_panes = { { id, title, weight? }, ... }",
        "Defines the initial plot panes. IDs are stable Lua keys, \
         titles are displayed in the GUI, and positive weights set \
         relative heights. The first pane is the default.",
    );

    ui.label(
        "If plot_panes is omitted, the compatible single default pane \
         is created. An explicitly named unknown pane is an error.",
    );

    section(ui, "Serial connections");

    ui.label(
        "Every entry in the connections table creates \
         one independent acquisition worker and one \
         serial connection.",
    );

    ui.label(
        "Commands to instruments on the same \
         connection are executed sequentially. \
         Different connections are processed by \
         independent worker threads.",
    );

    ui.label(
        "A non-empty connections table must contain \
         a connection named 'primary'. Other names \
         may be chosen freely and are used by the \
         Lua API.",
    );

    reference(ui, "port", "Required COM port name.");

    reference(ui, "baud_rate", "Baud rate. Default: 9600.");

    reference(ui, "data_bits", "Data bits: 5, 6, 7 or 8. Default: 8.");

    reference(
        ui,
        "parity",
        "\"none\", \"even\" or \"odd\". Default: \
         \"none\".",
    );

    reference(ui, "stop_bits", "One or two stop bits. Default: 1.");

    reference(
        ui,
        "flow_control",
        "\"none\", \"software\" or \"hardware\". \
         Default: \"none\".",
    );

    reference(ui, "timeout", "Read timeout in seconds. Default: 0.25.");

    section(ui, "Setup function");

    reference(
        ui,
        "setup = function() ... end",
        "Runs once after the application Lua API has \
         been installed. It may add series, start the \
         emulator, start acquisition or define global \
         REPL helpers.",
    );

    ui.label(
        "If setup fails, the Lua runtime reports the \
         error and is disconnected. Commands sent \
         before the failure may already have reached \
         the application.",
    );

    reference(
        ui,
        "scripts = { \"path.lua\", ... }",
        "Runs application scripts in the listed order \
         after setup() succeeds. A script may configure \
         an experiment, add series, define REPL helpers, \
         or register declarative control panels.",
    );

    section(ui, "Lua REPL");

    ui.label(
        "The REPL and files selected with Run script \
         share one persistent Lua runtime.",
    );

    ui.label(
        "Variables and functions remain available \
         between commands and executed scripts.",
    );

    ui.label(
        "Press Ctrl+Enter or click Execute to run the \
         current multiline input.",
    );

    ui.label(
        "Returned values and Lua errors appear in the \
         REPL history. Application actions are also \
         written to the application log.",
    );

    section(ui, "Application commands");

    reference(
        ui,
        "app.start()",
        "Starts periodic acquisition on every \
         configured connection.",
    );

    reference(
        ui,
        "app.stop()",
        "Stops periodic acquisition on every \
         configured connection.",
    );

    reference(
        ui,
        "app.clear()",
        "Removes all series and accumulated samples.",
    );

    reference(
        ui,
        "app.delete(name)",
        "Deletes one series by its unique name.",
    );

    reference(
        ui,
        "app.rename(current_name, new_name)",
        "Renames an existing series.",
    );

    reference(
        ui,
        "app.retry(name)",
        "Re-enables periodic polling for one suspended \
         series. The next request follows its normal \
         polling schedule.",
    );

    reference(
        ui,
        "app.retry_all()",
        "Re-enables periodic polling for every suspended \
         series without restarting acquisition.",
    );

    reference(
        ui,
        "app.log(message)",
        "Writes an informational message to the \
         application log.",
    );

    reference(
        ui,
        "app.start_emu()",
        "Starts the emulator configured in startup.lua.",
    );

    reference(ui, "app.stop_emu()", "Stops the running emulator.");

    section(ui, "Series options");

    ui.label(
        "A series is marked Offline after three \
         consecutive failed polling cycles. Existing \
         samples remain available. A successful manual \
         read or write of the same instrument parameter \
         also restores its polling.",
    );

    ui.label(
        "A series argument may be omitted, specified \
         as a name string, or specified as an options \
         table.",
    );

    reference(ui, "name", "Optional unique series name.");

    reference(
        ui,
        "interval",
        "Optional polling interval in seconds. The \
         application default is used when omitted.",
    );

    reference(
        ui,
        "color",
        "Optional line color in strict #RRGGBB format. \
         An automatic color is selected when omitted.",
    );

    reference(
        ui,
        "visible",
        "Optional initial plot visibility. Default: true.",
    );

    reference(
        ui,
        "pane",
        "Optional plot pane id from plot_panes. The first pane is used \
         when omitted.",
    );

    reference(
        ui,
        "app.set_color(name, color)",
        "Changes an existing series color. Pass nil to \
         restore automatic color selection.",
    );

    reference(
        ui,
        "app.set_series_pane(name, pane)",
        "Moves an existing series to a configured plot pane.",
    );

    code(ui, SERIES_COLOR_EXAMPLE);

    ui.label(
        "Text-command serial series additionally \
         accept the connection option.",
    );

    code(ui, SERIAL_SERIES_EXAMPLE);

    section(ui, "Text serial commands");

    reference(
        ui,
        "app.add_serial(command, options)",
        "Adds a periodically sampled text command. \
         Its response must contain one finite number.",
    );

    reference(
        ui,
        "app.send_serial(command, options)",
        "Sends one text command immediately and writes \
         its response or error to the application log.",
    );

    code(
        ui,
        r#"app.send_serial(
    "status",
    {
        connection = "primary",
    }
)"#,
    );

    section(ui, "Signal filters");

    reference(
        ui,
        "app.filter(input_name, options)",
        "Creates a derived series. The required options are name, kind and the selected filter's parameter.",
    );

    reference(
        ui,
        "app.set_filter(name, definition)",
        "Replaces a filtered series definition, resets that filter and resynchronizes downstream controllers.",
    );

    ui.label(
        "Supported filters are exponential (time_constant), moving_average (window) and median (an odd window). New filtered series may also specify color, visible and pane.",
    );

    code(ui, FILTER_EXAMPLE);

    section(ui, "Metakon 5X3");

    reference(
        ui,
        "app.metakon(options)",
        "Creates a typed Metakon 5X3 controller.",
    );

    ui.label(
        "Controller options are connection, device, \
         channel and scale. Defaults are primary, 1, \
         0 and 1.0.",
    );

    reference(
        ui,
        "controller:parameters()",
        "Returns typed parameter descriptors, access \
         modes, ranges and effective scales.",
    );

    reference(
        ui,
        "controller:add(parameter, options)",
        "Adds a readable parameter as a periodic \
         series.",
    );

    reference(
        ui,
        "controller:read(parameter)",
        "Performs one queued read and returns a number \
         or Boolean value.",
    );

    reference(
        ui,
        "controller:write(parameter, value)",
        "Performs one queued write, reads the parameter \
         back and returns its actual value.",
    );

    ui.label(
        "Periodic polling that is already due has \
         priority over interactive reads and writes.",
    );

    ui.label(
        "For the measurement parameter, the Metakon alarm \
         value -32768 is treated as a failed poll rather than \
         a temperature. No sample is stored. After three \
         consecutive alarm values, only the temperature series \
         is marked Offline; other readable parameters continue \
         to be polled.",
    );

    ui.label(
        "After the sensor fault is removed, read measurement \
         successfully or call app.retry() for the temperature \
         series. Calling app.retry_all() re-enables every \
         suspended series.",
    );

    ui.label(
        "A Refresh callback may read measurement and output_power \
         without displaying them as controls. Successful reads still \
         restore the matching suspended series, so temperature and \
         power may remain available only on the plot.",
    );

    ui.label(
        "Available parameters: channel_type, \
         measurement, setpoint, proportional_band, \
         integral_time, derivative_time, output_power, \
         pwm_positive, pwm_negative, upper_setpoint, \
         upper_hysteresis, upper_output, \
         lower_setpoint, lower_hysteresis and \
         lower_output.",
    );

    ui.label(
        "integral_time is exposed in minutes. The driver \
         converts the raw register value from seconds when \
         reading and back to seconds when writing. The \
         controller's scale option does not affect this \
         conversion.",
    );

    ui.label(
        "The Metakon front-panel OFF state for the integral \
         component is not reported by the integral_time \
         register; reading it returns the last stored numeric \
         value.",
    );

    code(ui, METAKON_EXAMPLE);
    code(ui, METAKON_REPL_EXAMPLE);

    section(ui, "Virtual instruments");

    reference(
        ui,
        "app.virtual_instrument(options)",
        "Discovers a virtual instrument through the \
         selected connection. The emulator or another \
         compatible server must already be running.",
    );

    ui.label(
        "Options are connection and the one-based \
         instrument id. Both default to primary and 1.",
    );

    reference(
        ui,
        "instrument:id()",
        "Returns the one-based instrument ID.",
    );

    reference(
        ui,
        "instrument:name()",
        "Returns the model-defined instrument name.",
    );

    reference(
        ui,
        "instrument:parameters()",
        "Returns discovered parameter descriptors.",
    );

    reference(
        ui,
        "instrument:add(parameter, options)",
        "Adds a readable parameter marked as \
         series-enabled.",
    );

    reference(
        ui,
        "instrument:read(parameter)",
        "Reads one parameter immediately.",
    );

    reference(
        ui,
        "instrument:write(parameter, value)",
        "Writes one parameter and returns the actual \
         value returned by the model.",
    );

    code(ui, VIRTUAL_INSTRUMENT_EXAMPLE);

    section(ui, "PID and on/off controllers");

    ui.label(
        "Create a controller from a writable Metakon or virtual-instrument parameter with instrument:pid(parameter, options) or instrument:on_off(parameter, options). The input option names an existing raw or filtered series.",
    );

    ui.label(
        "PID requires name, input, setpoint, kp, output_min and output_max; ki and kd default to zero. On/off requires name, input, setpoint, hysteresis, output_off and output_on. safe_output is optional and must fit the output parameter range.",
    );

    code(ui, PID_EXAMPLE);
    code(ui, ON_OFF_EXAMPLE);

    reference(
        ui,
        "controller:name()",
        "Returns the unique controller name.",
    );

    reference(
        ui,
        "controller:parameters(), diagnostics()",
        "Returns the parameters and diagnostic keys supported by the concrete controller type.",
    );

    ui.label(
        "PID parameters are setpoint, kp, ki, kd, output_min and output_max. On/off parameters are setpoint, hysteresis, output_off and output_on. PID diagnostics additionally include proportional, integral, derivative and unconstrained_output; both types expose setpoint and output.",
    );

    reference(
        ui,
        "controller:read/write/configure",
        "Reads or writes one parameter, or applies several parameter changes atomically.",
    );

    reference(
        ui,
        "controller:add(diagnostic, options)",
        "Adds an event-driven diagnostic series. It accepts name, color, visible and pane, but no polling interval.",
    );

    reference(
        ui,
        "controller:reference_kind(), reference_parameters()",
        "Describes the active fixed or ramp reference and its editable parameters.",
    );

    reference(
        ui,
        "controller:read_reference/write_reference/configure_reference",
        "Reads, writes or atomically configures reference parameters.",
    );

    reference(
        ui,
        "controller:set_fixed_reference(value)",
        "Replaces the active reference with a fixed value.",
    );

    reference(
        ui,
        "controller:set_ramp_reference({ start, target, rate })",
        "Replaces the active reference with a time-based ramp.",
    );

    reference(
        ui,
        "controller:set_input(name)",
        "Switches the controller to another existing series and resynchronizes its input timing.",
    );

    reference(
        ui,
        "controller:state/pause/resume/reset",
        "Inspects or controls the lifecycle. pause attempts the configured safe output; resume returns output ownership to automatic control.",
    );

    reference(
        ui,
        "controller:reset_integral()",
        "Clears only the integral term. This operation is supported by PID-based controllers, but not by on/off controllers.",
    );

    section(ui, "Application scripts and control panels");

    ui.label(
        "A script run from the startup profile or with Run \
         script may publish one or more declarative panels. \
         Until a script registers a panel, the Control panel \
         menu button remains disabled.",
    );

    reference(
        ui,
        "app.register_script(script)",
        "Registers a script table and publishes its panels. \
         The table requires a unique id; panels require id, \
         title and controls fields.",
    );

    reference(
        ui,
        "app.unregister_script(script_id)",
        "Removes the registered script and all of its \
         panels.",
    );

    reference(
        ui,
        "app.set_control(script_id, panel_id, control_id, value)",
        "Updates a readout, number or toggle from Lua. \
         Buttons do not store a value.",
    );

    reference(
        ui,
        "app.set_control_enabled(script_id, panel_id, control_id, enabled, reason?)",
        "Enables or disables a control and optionally shows a reason. \
         This is a GUI guard; equipment safety remains enforced by the \
         control and output subsystems.",
    );

    ui.label(
        "Control kinds are readout, number, toggle and \
         button. Number and toggle controls use on_change; \
         buttons use on_click. Callback names must refer to \
         functions stored in the registered script table.",
    );

    ui.label(
        "A number control is submitted after dragging stops \
         or keyboard editing loses focus, so partially typed \
         numbers are not sent. The callback should write the \
         value, read back the actual device value and update \
         the control with app.set_control().",
    );

    ui.label(
        "The Control panel opens as a separate native window \
         and is closed by default. Closing it does not \
         unregister the script or stop acquisition.",
    );

    code(ui, CONTROL_PANEL_EXAMPLE);

    section(ui, "Event-driven scenarios");

    reference(
        ui,
        "app.scenario({ id = id })",
        "Creates a scenario, or restarts the existing scenario with the \
         same id and cancels its old tasks.",
    );

    reference(
        ui,
        "scenario:after(seconds, callback)",
        "Runs a named Lua callback once after a monotonic relative delay.",
    );

    reference(
        ui,
        "scenario:at(unix_timestamp, callback)",
        "Runs a named callback once at an absolute UTC Unix timestamp in seconds.",
    );

    reference(
        ui,
        "scenario:when(condition, callback)",
        "Runs once when a series is above or below a threshold. Optional \
         for_seconds requires a continuous duration; hysteresis controls \
         rearming; edge currently accepts only \"rising\".",
    );

    reference(
        ui,
        "scenario:cancel(), scenario:id()",
        "Cancels all pending tasks, or returns the stable scenario id.",
    );

    ui.label(
        "Callbacks must be short global functions or unambiguous named \
         functions in a registered application script. Rust evaluates \
         timers and measurements; Lua only issues application commands.",
    );

    ui.label(
        "Callbacks are serialized per scenario. Recorded actions must be \
         Applied before another callback runs. A callback error or Failed \
         action stops the scenario, and every transition is written to the \
         process log. Reloading a profile cancels all scenarios.",
    );

    code(ui, SCENARIO_EXAMPLE);

    section(ui, "Virtual instrument models");

    ui.label(
        "The emulator model path is selected by the \
         emulator.script field in startup.lua.",
    );

    ui.label(
        "A model must define a global instruments \
         array. Array positions become one-based \
         instrument IDs.",
    );

    ui.label(
        "Every instrument contains a name and a \
         non-empty parameters array.",
    );

    ui.label(
        "Parameter fields are key, name, type, access, \
         series, unit, min and max. Name defaults to \
         key, access defaults to read_only and series \
         defaults to false.",
    );

    ui.label(
        "Supported types are boolean, integer and \
         number. Supported access modes are read_only, \
         write_only and read_write. Min and max must \
         either both be present or both be absent.",
    );

    reference(
        ui,
        "read(instrument_id, parameter, time)",
        "Required when the model has at least one \
         readable parameter. Time is elapsed seconds \
         since emulator startup.",
    );

    reference(
        ui,
        "write(instrument_id, parameter, value, time)",
        "Required when the model has at least one \
         writable parameter. It must return the actual \
         stored value.",
    );

    code(ui, VIRTUAL_MODEL_EXAMPLE);

    section(ui, "Plot controls");

    ui.label(
        "Use Add plot and Remove last plot to change the \
         number of panes. Drag the separator between panes to \
         change their relative heights. The proportions are \
         preserved while the window is resized.",
    );

    ui.label(
        "A startup profile may define the initial panes declaratively with \
         plot_panes. Series options accept visible and pane, and \
         app.set_series_pane() moves a series later.",
    );

    ui.label(
        "Use the series side panel to select visibility and \
         assign each series to a plot pane. Double-click a \
         plot to resume following the latest data and restore \
         automatic Y bounds.",
    );

    section(ui, "Process database");

    ui.label(
        "A new timestamped SQLite process database \
         is created automatically on every application \
         launch under processes/YYYY-MM-DD in the \
         application directory.",
    );

    ui.label(
        "Every successful periodic measurement is \
         recorded automatically. No explicit recording \
         command is required.",
    );

    ui.label(
        "The database also records the loaded \
         configuration source, application log and \
         requested actions. Its path is written to the \
         application log.",
    );

    ui.label(
        "If SQLite cannot be opened or writing fails, \
         the application continues running and reports \
         that process recording is disabled.",
    );
}
