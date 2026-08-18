use eframe::egui;

use super::help_model::{HelpLanguage, HelpModel};

const STARTUP_EXAMPLE: &str = r#"local definition = {
    application = {
        fps = 20,
        poll_interval = 1.0,
        plot_window = 3600.0,
        max_plot_points_per_series = 1000,
    },

    connections = {
        primary = {
            port = "COM3",
            baud_rate = 9600,
            data_bits = 8,
            parity = "none",
            stop_bits = 1,
            flow_control = "none",
            timeout = 0.25,
        },
    },

    emulator = {
        connection = "primary",
        port = "COM4",
        script = "emulator_scripts/sine_generator.lua",
    },

    scripts = {
        "lua_scripts/experiment.lua",
    },
}

function definition.setup()
    app.log("Application initialized.")
end

return definition"#;

const SERIAL_SERIES_EXAMPLE: &str = r##"app.add_serial(
    "read temperature",
    {
        name = "temperature",
        connection = "primary",
        interval = 0.5,
        color = "#1976D2",
    }
)"##;

const METAKON_EXAMPLE: &str = r##"controller = app.metakon({
    connection = "primary",
    device = 15,
    channel = 0,
    scale = 1.0,
})

controller:add(
    "measurement",
    {
        name = "temperature",
        interval = 1.0,
        color = "#D32F2F",
    }
)

controller:add("setpoint", "setpoint")
controller:add("output_power", "power")

app.start()"##;

const METAKON_REPL_EXAMPLE: &str = r#"controller:read("measurement")
controller:write("setpoint", 150)
controller:write("proportional_band", 20)"#;

const VIRTUAL_INSTRUMENT_EXAMPLE: &str = r##"app.start_emu()

generator = app.virtual_instrument({
    connection = "primary",
    id = 1,
})

generator:write("amplitude", 100.0)
generator:write("period", 300.0)
generator:write("phase", 0.0)

generator:add(
    "value",
    {
        name = "virtual_sine",
        interval = 0.25,
        color = "#35B779",
    }
)

app.start()"##;

const VIRTUAL_MODEL_EXAMPLE: &str = r#"local amplitude = 1.0

instruments = {
    {
        name = "Generator",

        parameters = {
            {
                key = "value",
                name = "Signal value",
                type = "number",
                access = "read_only",
                series = true,
                unit = "V",
                min = -1000.0,
                max = 1000.0,
            },

            {
                key = "amplitude",
                name = "Amplitude",
                type = "number",
                access = "read_write",
                min = 0.0,
                max = 1000.0,
            },
        },
    },
}

function read(
    instrument_id,
    parameter,
    time
)
    if parameter == "value" then
        return amplitude * math.sin(time)
    end

    if parameter == "amplitude" then
        return amplitude
    end

    error("unknown parameter: " .. parameter)
end

function write(
    instrument_id,
    parameter,
    value,
    time
)
    if parameter == "amplitude" then
        amplitude = value
        return amplitude
    end

    error("parameter is not writable: " .. parameter)
end"#;

const SERIES_COLOR_EXAMPLE: &str = r##"app.set_color("temperature", "#D32F2F")
app.set_color("temperature", nil) -- restore automatic color"##;

const CONTROL_PANEL_EXAMPLE: &str = r#"local controller = app.metakon({
    connection = "primary",
    device = 15,
    channel = 0,
})

local script = {
    id = "heater_control",

    panels = {
        {
            id = "heater",
            title = "Heater",
            controls = {
                {
                    kind = "readout",
                    id = "temperature",
                    label = "Temperature",
                    initial = "—",
                },
                {
                    kind = "number",
                    id = "setpoint",
                    label = "Setpoint",
                    initial = 20.0,
                    min = 0.0,
                    max = 400.0,
                    step = 1.0,
                    on_change = "set_setpoint",
                },
                {
                    kind = "button",
                    id = "refresh",
                    label = "Refresh",
                    on_click = "refresh",
                },
            },
        },
    },
}

function script.set_setpoint(value)
    local actual = controller:write("setpoint", value)
    app.set_control(script.id, "heater", "setpoint", actual)
end

function script.refresh()
    local value = controller:read("measurement")
    app.set_control(
        script.id,
        "heater",
        "temperature",
        string.format("%.1f °C", value)
    )
end

app.register_script(script)"#;

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
                            show_english_reference(ui);
                        }

                        HelpLanguage::Russian => {
                            show_russian_reference(ui);
                        }
                    }
                });
        });

    model.set_language(language);
    model.set_command_reference_open(open);
}

fn show_english_reference(ui: &mut egui::Ui) {
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
         connections, emulator, scripts and setup.",
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
        "app.set_color(name, color)",
        "Changes an existing series color. Pass nil to \
         restore automatic color selection.",
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

fn show_russian_reference(ui: &mut egui::Ui) {
    ui.heading("Среды Lua");

    ui.label(
        "Приложение использует две независимые среды \
         Lua.",
    );

    ui.label(
        "Сценарии приложения и REPL управляют опросом, \
         сериями, приборами и эмулятором через \
         глобальную таблицу 'app'.",
    );

    ui.label(
        "Сценарии моделей выполняются внутри эмулятора \
         и описывают виртуальные приборы. Таблица 'app' \
         в них недоступна.",
    );

    section(ui, "Конфигурация приложения");

    ui.label(
        "Приложение загружает выбранный стартовый профиль. \
         Если профиль не выбран, используется startup.lua \
         из каталога приложения. Сценарий должен вернуть \
         одну таблицу.",
    );

    ui.label(
        "Поддерживаются корневые разделы application, \
         connections, emulator, scripts и setup.",
    );

    ui.label(
        "Верхний уровень startup.lua не должен иметь \
         побочных эффектов: он выполняется один раз \
         для проверки и ещё раз в рабочей среде Lua. \
         Действия приложения помещайте в setup().",
    );

    code(ui, STARTUP_EXAMPLE);

    ui.label(
        "Относительные пути к сценариям и модели \
         эмулятора отсчитываются от каталога выбранного \
         стартового профиля. Журналы, базы процесса и \
         другие данные сохраняются относительно каталога \
         приложения.",
    );

    ui.label(
        "В Settings можно выбрать и проверить другой \
         Lua-профиль перед загрузкой. Активный профиль \
         запоминается для следующего запуска. Параметр \
         командной строки --config явно выбирает профиль.",
    );

    ui.label(
        "Загрузка или перезагрузка профиля полностью заменяет \
         рабочую среду: опрос и эмулятор останавливаются, \
         панели управления удаляются, а серии и история \
         графиков очищаются.",
    );

    section(ui, "Параметры приложения");

    reference(
        ui,
        "fps",
        "Частота перерисовки интерфейса от 1 до 240 \
         кадров в секунду. По умолчанию: 30.",
    );

    reference(
        ui,
        "poll_interval",
        "Интервал опроса серий по умолчанию в секундах. \
         По умолчанию: 1.0.",
    );

    reference(
        ui,
        "plot_window",
        "Размер текущего окна графика в секундах. \
         По умолчанию: 3600.0.",
    );

    reference(
        ui,
        "max_plot_points_per_series",
        "Максимальное число подготовленных точек одной \
         видимой серии. По умолчанию: 4000.",
    );

    section(ui, "Последовательные подключения");

    ui.label(
        "Каждая запись в таблице connections создаёт \
         независимый worker и отдельное подключение к \
         последовательному порту.",
    );

    ui.label(
        "Команды приборам одного подключения \
         выполняются последовательно. Разные \
         подключения обслуживаются независимыми \
         потоками.",
    );

    ui.label(
        "Непустая таблица connections должна содержать \
         подключение с именем 'primary'. Остальные \
         имена выбираются произвольно и используются \
         в Lua API.",
    );

    reference(ui, "port", "Обязательное имя COM-порта.");

    reference(ui, "baud_rate", "Скорость обмена. По умолчанию: 9600.");

    reference(
        ui,
        "data_bits",
        "Число бит данных: 5, 6, 7 или 8. По \
         умолчанию: 8.",
    );

    reference(
        ui,
        "parity",
        "\"none\", \"even\" или \"odd\". По умолчанию: \
         \"none\".",
    );

    reference(
        ui,
        "stop_bits",
        "Один или два стоповых бита. По умолчанию: 1.",
    );

    reference(
        ui,
        "flow_control",
        "\"none\", \"software\" или \"hardware\". По \
         умолчанию: \"none\".",
    );

    reference(
        ui,
        "timeout",
        "Тайм-аут чтения в секундах. По умолчанию: \
         0.25.",
    );

    section(ui, "Функция setup");

    reference(
        ui,
        "setup = function() ... end",
        "Вызывается один раз после установки Lua API. \
         Может добавлять серии, запускать эмулятор и \
         опрос, а также объявлять глобальные функции \
         для REPL.",
    );

    ui.label(
        "Если setup завершится ошибкой, среда Lua \
         сообщит об ошибке и отключится. Команды, \
         отправленные до ошибки, уже могли попасть в \
         приложение.",
    );

    reference(
        ui,
        "scripts = { \"path.lua\", ... }",
        "После успешного setup() выполняет сценарии \
         приложения в указанном порядке. Сценарий может \
         настраивать эксперимент, добавлять серии, создавать \
         функции для REPL и регистрировать панели управления.",
    );

    section(ui, "Lua REPL");

    ui.label(
        "REPL и файлы, выбранные кнопкой Run script, \
         используют одну сохраняющую состояние среду \
         Lua.",
    );

    ui.label(
        "Переменные и функции остаются доступны между \
         командами и запусками сценариев.",
    );

    ui.label(
        "Для выполнения многострочного ввода нажмите \
         Ctrl+Enter или кнопку Execute.",
    );

    ui.label(
        "Возвращённые значения и ошибки Lua появляются \
         в истории REPL. Действия с приложением также \
         записываются в журнал.",
    );

    section(ui, "Команды приложения");

    reference(
        ui,
        "app.start()",
        "Запускает периодический опрос всех настроенных \
         подключений.",
    );

    reference(
        ui,
        "app.stop()",
        "Останавливает периодический опрос всех \
         настроенных подключений.",
    );

    reference(ui, "app.clear()", "Удаляет все серии и накопленные точки.");

    reference(
        ui,
        "app.delete(name)",
        "Удаляет одну серию по уникальному имени.",
    );

    reference(
        ui,
        "app.rename(current_name, new_name)",
        "Переименовывает существующую серию.",
    );

    reference(
        ui,
        "app.retry(name)",
        "Возобновляет периодический опрос одной \
         отключённой серии. Следующий запрос выполняется \
         по её обычному расписанию.",
    );

    reference(
        ui,
        "app.retry_all()",
        "Возобновляет опрос всех отключённых серий без \
         перезапуска сбора данных.",
    );

    reference(
        ui,
        "app.log(message)",
        "Записывает информационное сообщение в журнал \
         приложения.",
    );

    reference(
        ui,
        "app.start_emu()",
        "Запускает эмулятор, описанный в startup.lua.",
    );

    reference(ui, "app.stop_emu()", "Останавливает работающий эмулятор.");

    section(ui, "Параметры серий");

    ui.label(
        "После трёх последовательных неудачных циклов \
         опроса серия получает состояние Offline. Уже \
         накопленные точки сохраняются. Успешное ручное \
         чтение или изменение того же параметра прибора \
         также восстанавливает опрос.",
    );

    ui.label(
        "Аргумент серии можно опустить, передать как \
         строку с именем или как таблицу параметров.",
    );

    reference(ui, "name", "Необязательное уникальное имя серии.");

    reference(
        ui,
        "interval",
        "Необязательный интервал опроса в секундах. При \
         отсутствии используется интервал приложения.",
    );

    reference(
        ui,
        "color",
        "Необязательный цвет линии в строгом формате \
         #RRGGBB. Если цвет не указан, он выбирается \
         автоматически.",
    );

    reference(
        ui,
        "app.set_color(name, color)",
        "Меняет цвет существующей серии. Передайте nil, \
         чтобы снова выбирать цвет автоматически.",
    );

    code(ui, SERIES_COLOR_EXAMPLE);

    ui.label(
        "Текстовые серии последовательного порта также \
         принимают параметр connection.",
    );

    code(ui, SERIAL_SERIES_EXAMPLE);

    section(ui, "Текстовые команды порта");

    reference(
        ui,
        "app.add_serial(command, options)",
        "Добавляет периодически выполняемую текстовую \
         команду. Ответ должен содержать одно конечное \
         число.",
    );

    reference(
        ui,
        "app.send_serial(command, options)",
        "Однократно отправляет текстовую команду и \
         записывает ответ или ошибку в журнал.",
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

    section(ui, "МЕТАКОН 5X3");

    reference(
        ui,
        "app.metakon(options)",
        "Создаёт типизированный контроллер МЕТАКОН \
         5X3.",
    );

    ui.label(
        "Параметры контроллера: connection, device, \
         channel и scale. Значения по умолчанию: \
         primary, 1, 0 и 1.0.",
    );

    reference(
        ui,
        "controller:parameters()",
        "Возвращает типизированные описания параметров, \
         режимы доступа, диапазоны и масштабы.",
    );

    reference(
        ui,
        "controller:add(parameter, options)",
        "Добавляет читаемый параметр как периодическую \
         серию.",
    );

    reference(
        ui,
        "controller:read(parameter)",
        "Выполняет одно чтение из очереди и возвращает \
         число или логическое значение.",
    );

    reference(
        ui,
        "controller:write(parameter, value)",
        "Выполняет запись, читает параметр обратно и \
         возвращает фактическое значение.",
    );

    ui.label(
        "Периодический опрос, срок которого уже \
         наступил, имеет приоритет перед разовыми \
         чтениями и записями.",
    );

    ui.label(
        "Для параметра measurement аварийное значение \
         МЕТАКОНа -32768 считается ошибкой опроса, а не \
         температурой. Точка не сохраняется. После трёх \
         последовательных аварийных значений состояние Offline \
         получает только серия температуры; остальные \
         доступные параметры продолжают опрашиваться.",
    );

    ui.label(
        "После устранения неисправности датчика успешно \
         прочитайте measurement или вызовите app.retry() для \
         серии температуры. app.retry_all() возобновляет все \
         приостановленные серии.",
    );

    ui.label(
        "Обработчик Refresh может читать measurement и output_power, \
         не создавая для них элементов панели. Успешное чтение всё \
         равно восстанавливает соответствующую приостановленную серию, \
         поэтому температура и мощность могут отображаться только на \
         графике.",
    );

    ui.label(
        "Доступные параметры: channel_type, \
         measurement, setpoint, proportional_band, \
         integral_time, derivative_time, output_power, \
         pwm_positive, pwm_negative, upper_setpoint, \
         upper_hysteresis, upper_output, \
         lower_setpoint, lower_hysteresis и \
         lower_output.",
    );

    ui.label(
        "integral_time передаётся в минутах. При чтении \
         драйвер преобразует сырое значение регистра из \
         секунд, а при записи — обратно в секунды. Параметр \
         scale контроллера на это преобразование не влияет.",
    );

    ui.label(
        "Состояние OFF интегральной составляющей на панели \
         МЕТАКОНа не передаётся регистром integral_time: его \
         чтение возвращает последнее сохранённое числовое \
         значение.",
    );

    code(ui, METAKON_EXAMPLE);
    code(ui, METAKON_REPL_EXAMPLE);

    section(ui, "Виртуальные приборы");

    reference(
        ui,
        "app.virtual_instrument(options)",
        "Обнаруживает виртуальный прибор через \
         выбранное подключение. Эмулятор или другой \
         совместимый сервер уже должен работать.",
    );

    ui.label(
        "Параметры: connection и нумеруемый с единицы \
         идентификатор прибора. По умолчанию \
         используются primary и 1.",
    );

    reference(ui, "instrument:id()", "Возвращает идентификатор прибора.");

    reference(ui, "instrument:name()", "Возвращает имя прибора из модели.");

    reference(
        ui,
        "instrument:parameters()",
        "Возвращает обнаруженные описания параметров.",
    );

    reference(
        ui,
        "instrument:add(parameter, options)",
        "Добавляет читаемый параметр, разрешённый для \
         построения серии.",
    );

    reference(
        ui,
        "instrument:read(parameter)",
        "Однократно читает параметр.",
    );

    reference(
        ui,
        "instrument:write(parameter, value)",
        "Записывает параметр и возвращает фактическое \
         значение из модели.",
    );

    code(ui, VIRTUAL_INSTRUMENT_EXAMPLE);

    section(ui, "Сценарии приложения и панели управления");

    ui.label(
        "Сценарий из стартового профиля или запущенный \
         кнопкой Run script может опубликовать одну или \
         несколько декларативных панелей. Пока ни один \
         сценарий не зарегистрировал панель, кнопка Control \
         panel недоступна.",
    );

    reference(
        ui,
        "app.register_script(script)",
        "Регистрирует таблицу сценария и публикует её панели. \
         Таблице нужен уникальный id; панели содержат id, \
         title и controls.",
    );

    reference(
        ui,
        "app.unregister_script(script_id)",
        "Удаляет зарегистрированный сценарий и все его \
         панели.",
    );

    reference(
        ui,
        "app.set_control(script_id, panel_id, control_id, value)",
        "Обновляет из Lua поле readout, number или toggle. \
         Кнопки не хранят значение.",
    );

    ui.label(
        "Поддерживаются элементы readout, number, toggle и \
         button. Для number и toggle используется on_change, \
         для button — on_click. Имена обработчиков должны \
         указывать на функции в зарегистрированной таблице \
         сценария.",
    );

    ui.label(
        "Числовое поле отправляет значение после окончания \
         перетаскивания или потери фокуса при вводе с \
         клавиатуры, поэтому неполное число не попадает в \
         прибор. Обработчику лучше записать значение, прочитать \
         фактический результат и обновить поле через \
         app.set_control().",
    );

    ui.label(
        "Control panel открывается отдельным системным окном \
         и по умолчанию закрыта. Закрытие окна не отменяет \
         регистрацию сценария и не останавливает опрос.",
    );

    code(ui, CONTROL_PANEL_EXAMPLE);

    section(ui, "Модели виртуальных приборов");

    ui.label(
        "Путь к модели задаётся полем emulator.script \
         файла startup.lua.",
    );

    ui.label(
        "Модель должна определить глобальный массив \
         instruments. Позиции в массиве становятся \
         идентификаторами приборов, начиная с единицы.",
    );

    ui.label(
        "Каждый прибор содержит имя и непустой массив \
         parameters.",
    );

    ui.label(
        "Поля параметра: key, name, type, access, \
         series, unit, min и max. По умолчанию name \
         совпадает с key, access равен read_only, а \
         series равен false.",
    );

    ui.label(
        "Типы: boolean, integer и number. Режимы \
         доступа: read_only, write_only и read_write. \
         Поля min и max должны присутствовать либо \
         вместе, либо отсутствовать вместе.",
    );

    reference(
        ui,
        "read(instrument_id, parameter, time)",
        "Обязательна при наличии хотя бы одного \
         читаемого параметра. time — время в секундах \
         с момента запуска эмулятора.",
    );

    reference(
        ui,
        "write(instrument_id, parameter, value, time)",
        "Обязательна при наличии хотя бы одного \
         записываемого параметра. Должна вернуть \
         фактически сохранённое значение.",
    );

    code(ui, VIRTUAL_MODEL_EXAMPLE);

    section(ui, "Управление графиками");

    ui.label(
        "Кнопки Add plot и Remove last plot меняют число \
         панелей графиков. Перетаскивайте разделитель между \
         панелями, чтобы менять их относительную высоту. \
         Пропорции сохраняются при изменении размера окна.",
    );

    ui.label(
        "В боковой панели серий настраивается видимость и \
         выбирается панель графика для каждой серии. Двойной \
         щелчок по графику возвращает слежение за последними \
         данными и автоматические границы оси Y.",
    );

    section(ui, "База процесса");

    ui.label(
        "При каждом запуске приложения автоматически \
         создаётся новая база SQLite с временной меткой \
         в каталоге processes/YYYY-MM-DD внутри \
         каталога приложения.",
    );

    ui.label(
        "Каждое успешное периодическое измерение \
         записывается автоматически. Отдельно запускать \
         запись не требуется.",
    );

    ui.label(
        "В базу также попадают исходный текст \
         загруженной конфигурации, журнал приложения и \
         запрошенные действия. Путь к базе записывается \
         в журнал.",
    );

    ui.label(
        "Если SQLite открыть не удалось или при записи \
         произошла ошибка, приложение продолжает работу \
         и сообщает, что запись процесса отключена.",
    );
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
