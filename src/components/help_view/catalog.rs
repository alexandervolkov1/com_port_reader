//! A single bilingual catalog keeps signatures and examples identical in both Help languages.
use crate::components::help_model::{HelpCategory, HelpLanguage};

pub(super) struct Entry {
    pub category: HelpCategory,
    pub signature: &'static str,
    summary: [&'static str; 2],
    details: [&'static str; 2],
    pub example: &'static str,
}

impl Entry {
    pub fn summary(&self, language: HelpLanguage) -> &'static str {
        language.choose(self.summary[0], self.summary[1])
    }

    pub fn details(&self, language: HelpLanguage) -> &'static str {
        language.choose(self.details[0], self.details[1])
    }

    /// Search both translations plus syntax and examples, with AND semantics for query words.
    /// This lets a Russian reader find English API names without switching languages.
    pub fn matches(&self, category: HelpCategory, query: &str) -> bool {
        if category != HelpCategory::All && self.category != category {
            return false;
        }
        let haystack = format!(
            "{} {} {} {} {} {}",
            self.signature,
            self.summary[0],
            self.summary[1],
            self.details[0],
            self.details[1],
            self.example
        )
        .to_lowercase();
        query
            .split_whitespace()
            .all(|word| haystack.contains(&word.to_lowercase()))
    }
}

pub(super) const ENTRIES: &[Entry] = &[
    Entry {
        category: HelpCategory::Application,
        signature: r###"app.start()"###,
        summary: [
            r###"Start periodic acquisition."###,
            r###"Запустить периодический опрос."###,
        ],
        details: [
            r###"Queues Start for all connections. Errors appear in the log; existing series are retained."###,
            r###"Ставит Start в очередь всех соединений. Ошибки видны в журнале; существующие серии сохраняются."###,
        ],
        example: r###"app.start()"###,
    },
    Entry {
        category: HelpCategory::Application,
        signature: r###"app.stop()"###,
        summary: [
            r###"Stop periodic acquisition; keep history."###,
            r###"Остановить опрос, сохранив историю."###,
        ],
        details: [
            r###"Does not pause controllers or set actuators safe. Pause/remove controllers first when ending an experiment."###,
            r###"Не приостанавливает регуляторы и не переводит выходы в безопасное состояние. При завершении опыта сначала вызовите pause/remove."###,
        ],
        example: r###"app.stop()"###,
    },
    Entry {
        category: HelpCategory::Application,
        signature: r###"app.clear()"###,
        summary: [
            r###"Remove series, filters and controllers."###,
            r###"Удалить серии, фильтры и регуляторы."###,
        ],
        details: [
            r###"Coordinated removal can fail if a controller's safe write fails. Check the log."###,
            r###"Согласованное удаление может не завершиться при ошибке безопасной записи регулятора. Проверьте журнал."###,
        ],
        example: r###"app.clear()"###,
    },
    Entry {
        category: HelpCategory::Application,
        signature: r###"app.log(message)"###,
        summary: [
            r###"Write an informational log message."###,
            r###"Записать информационное сообщение в журнал."###,
        ],
        details: [
            r###"message: string. Also recorded in SQLite when available; no return value."###,
            r###"message: строка. При доступной SQLite сообщение также записывается в базу; результата нет."###,
        ],
        example: r###"app.log("Experiment started")"###,
    },
    Entry {
        category: HelpCategory::Application,
        signature: r###"app.start_emu()"###,
        summary: [
            r###"Start the model selected by the profile."###,
            r###"Запустить модель, выбранную в профиле."###,
        ],
        details: [
            r###"Memory transport requires no COM driver. Repeated Start is harmless; restart creates fresh model state."###,
            r###"Транспорт memory не требует COM-драйвера. Повторный Start безвреден; перезапуск создаёт новое состояние модели."###,
        ],
        example: r###"app.start_emu()"###,
    },
    Entry {
        category: HelpCategory::Application,
        signature: r###"app.stop_emu()"###,
        summary: [
            r###"Stop and join the emulator."###,
            r###"Остановить эмулятор и дождаться его завершения."###,
        ],
        details: [
            r###"Pause controllers first. Series remain, but virtual requests fail until restart."###,
            r###"Сначала приостановите регуляторы. Серии сохраняются, но запросы к модели до перезапуска завершаются ошибкой."###,
        ],
        example: r###"app.stop_emu()"###,
    },
    Entry {
        category: HelpCategory::Series,
        signature: r###"app.add_serial(command, options?)"###,
        summary: [
            r###"Plot a numeric text reply."###,
            r###"Построить график числового текстового ответа."###,
        ],
        details: [
            r###"command is sent with LF. options: name string or {name, interval, connection, color, visible, pane}. Reply must be finite numeric text."###,
            r###"command отправляется с LF. options: имя строкой или {name, interval, connection, color, visible, pane}. Ответ должен быть конечным числом."###,
        ],
        example: r###"app.add_serial("READ?", {name = "temperature", interval = 1})"###,
    },
    Entry {
        category: HelpCategory::Series,
        signature: r###"app.send_serial(command, options?)"###,
        summary: [
            r###"Send a one-shot text command."###,
            r###"Отправить разовую текстовую команду."###,
        ],
        details: [
            r###"options accepts connection only. No returned reply: the response or error appears in the log. Embedded CR/LF is forbidden."###,
            r###"options принимает только connection. Ответ не возвращается: он или ошибка появится в журнале. CR/LF внутри команды запрещены."###,
        ],
        example: r###"app.send_serial("STATUS?", {connection = "primary"})"###,
    },
    Entry {
        category: HelpCategory::Series,
        signature: r###"app.rename(name, new_name)"###,
        summary: [
            r###"Rename a series without losing its history."###,
            r###"Переименовать серию без потери истории."###,
        ],
        details: [
            r###"Both arguments are series-name strings. Later Lua lookups use the new name."###,
            r###"Оба аргумента — строки с именами серий. В последующих вызовах Lua используйте новое имя."###,
        ],
        example: r###"app.rename("temperature", "furnace_temperature")"###,
    },
    Entry {
        category: HelpCategory::Series,
        signature: r###"app.delete(name)"###,
        summary: [
            r###"Remove a series and dependent processing."###,
            r###"Удалить серию и зависимую обработку."###,
        ],
        details: [
            r###"Affected controller outputs are handled safely before removal; failure is reported in the log."###,
            r###"Перед удалением затронутые выходы регуляторов переводятся в безопасное состояние; ошибки видны в журнале."###,
        ],
        example: r###"app.delete("temperature_smooth")"###,
    },
    Entry {
        category: HelpCategory::Series,
        signature: r###"app.set_color(name, color)"###,
        summary: [r###"Change a line color."###, r###"Изменить цвет линии."###],
        details: [
            r###"color: #RRGGBB string; nil restores automatic color."###,
            r###"color: строка #RRGGBB; nil возвращает автоматический цвет."###,
        ],
        example: r###"app.set_color("temperature", "#FF8800")"###,
    },
    Entry {
        category: HelpCategory::Series,
        signature: r###"app.set_series_pane(name, pane)"###,
        summary: [
            r###"Move a series to a configured plot pane."###,
            r###"Перенести серию на заданный график."###,
        ],
        details: [
            r###"pane: required plot_panes ID string. The built-in default pane is main; profiles can define other IDs."###,
            r###"pane: обязательная строка ID из plot_panes. Встроенный график по умолчанию — main; профиль может задать другие ID."###,
        ],
        example: r###"app.set_series_pane("temperature", "main")"###,
    },
    Entry {
        category: HelpCategory::Series,
        signature: r###"app.retry(name)"###,
        summary: [
            r###"Retry a suspended raw series."###,
            r###"Возобновить опрос отключённой серии."###,
        ],
        details: [
            r###"After three consecutive poll failures a series goes Offline. Retry resets its polling failure history."###,
            r###"После трёх ошибок опроса подряд серия переходит в Offline. Retry сбрасывает историю ошибок опроса."###,
        ],
        example: r###"app.retry("temperature")"###,
    },
    Entry {
        category: HelpCategory::Series,
        signature: r###"app.retry_all()"###,
        summary: [
            r###"Retry all suspended raw series."###,
            r###"Возобновить опрос всех отключённых серий."###,
        ],
        details: [
            r###"Does not change device settings. Correct the connection/model failure before retrying."###,
            r###"Не меняет настройки прибора. Перед повтором устраните проблему соединения или модели."###,
        ],
        example: r###"app.retry_all()"###,
    },
    Entry {
        category: HelpCategory::Series,
        signature: r###"app.filter(input, options)"###,
        summary: [
            r###"Add a filtered signal."###,
            r###"Добавить отфильтрованный сигнал."###,
        ],
        details: [
            r###"options requires name and kind. moving_average: window 1..100000; median: odd window; exponential: positive time_constant in seconds. No interval."###,
            r###"options требует name и kind. moving_average: window 1..100000; median: нечётный window; exponential: положительная time_constant в секундах. interval не поддерживается."###,
        ],
        example: r###"app.filter("temperature", {name = "temperature_smooth",
    kind = "exponential", time_constant = 2})
-- Alternatives: kind = "moving_average", window = 5
--               kind = "median", window = 5"###,
    },
    Entry {
        category: HelpCategory::Series,
        signature: r###"app.set_filter(name, definition)"###,
        summary: [
            r###"Replace an existing filter's settings."###,
            r###"Изменить настройки существующего фильтра."###,
        ],
        details: [
            r###"definition: {kind, window} or {kind, time_constant}. Clears this and downstream filter history; controller time history is resynchronized."###,
            r###"definition: {kind, window} или {kind, time_constant}. Сбрасывает историю фильтра и зависимых фильтров; временная история регулятора синхронизируется заново."###,
        ],
        example: r###"app.set_filter("temperature_smooth", {kind = "median", window = 5})"###,
    },
    Entry {
        category: HelpCategory::Instruments,
        signature: r###"app.virtual_instrument(options?) → device"###,
        summary: [
            r###"Discover a virtual instrument."###,
            r###"Получить виртуальный прибор."###,
        ],
        details: [
            r###"options: {id = 1, connection = "primary"}; defaults shown. The model must already run. Discovery waits for the catalog."###,
            r###"options: {id = 1, connection = "primary"}; показаны значения по умолчанию. Модель должна быть запущена. Вызов ждёт каталог."###,
        ],
        example: r###"plant = app.virtual_instrument({id = 1})"###,
    },
    Entry {
        category: HelpCategory::Instruments,
        signature: r###"app.metakon(options?) → device"###,
        summary: [
            r###"Create a Metakon 5X3 handle."###,
            r###"Создать объект Metakon 5X3."###,
        ],
        details: [
            r###"Defaults: connection="primary", device=1, channel=0, scale=1. Addresses are bytes; scale is positive. Construction performs no I/O."###,
            r###"По умолчанию: connection="primary", device=1, channel=0, scale=1. Адреса — байты; scale положителен. Конструктор не обращается к прибору."###,
        ],
        example: r###"meter = app.metakon({device = 1, channel = 0, scale = 0.1})"###,
    },
    Entry {
        category: HelpCategory::Instruments,
        signature: r###"device:id() → integer"###,
        summary: [
            r###"Return the discovered virtual instrument ID."###,
            r###"Получить ID виртуального прибора."###,
        ],
        details: [
            r###"Virtual instruments only; one-based catalog ID. Metakon has no id() method."###,
            r###"Только виртуальные приборы; ID в каталоге начинается с 1. У Metakon метода id() нет."###,
        ],
        example: r###"return plant:id()"###,
    },
    Entry {
        category: HelpCategory::Instruments,
        signature: r###"device:name() → string"###,
        summary: [
            r###"Return the virtual instrument name."###,
            r###"Получить имя виртуального прибора."###,
        ],
        details: [
            r###"Virtual instruments only; Metakon has no name() method."###,
            r###"Только виртуальные приборы; у Metakon метода name() нет."###,
        ],
        example: r###"return plant:name()"###,
    },
    Entry {
        category: HelpCategory::Instruments,
        signature: r###"device:parameters() → descriptors"###,
        summary: [
            r###"List parameter keys, types, access and ranges."###,
            r###"Посмотреть ключи, типы, доступ и диапазоны параметров."###,
        ],
        details: [
            r###"Shared by virtual and Metakon handles. Use descriptor keys, not display labels, in read/write/add."###,
            r###"Общий метод виртуальных приборов и Metakon. В read/write/add передавайте key, а не отображаемое имя."###,
        ],
        example: r###"for _, parameter in ipairs(plant:parameters()) do
    app.log(parameter.key .. ": " .. parameter.value_type .. " / " .. parameter.access)
end"###,
    },
    Entry {
        category: HelpCategory::Instruments,
        signature: r###"device:add(parameter, options?)"###,
        summary: [
            r###"Add a periodically sampled instrument parameter."###,
            r###"Добавить периодический опрос параметра прибора."###,
        ],
        details: [
            r###"options: name string or {name, interval, color, visible, pane}. Metakon uses measurement; the supplied furnace uses temperature."###,
            r###"options: имя строкой или {name, interval, color, visible, pane}. У Metakon ключ measurement; у модели печи — temperature."###,
        ],
        example: r###"plant:add("temperature", {name = "temperature", interval = 0.5})
-- Metakon: meter:add("measurement", "temperature")"###,
    },
    Entry {
        category: HelpCategory::Instruments,
        signature: r###"device:read(parameter) → value"###,
        summary: [
            r###"Read one parameter and wait for the result."###,
            r###"Прочитать параметр и дождаться результата."###,
        ],
        details: [
            r###"Returns boolean/integer/number according to the descriptor. Errors raise Lua errors; a read alone creates no plotted series."###,
            r###"Возвращает boolean/integer/number согласно описанию. Ошибка вызывает ошибку Lua; чтение само по себе не создаёт серию."###,
        ],
        example: r###"return plant:read("temperature")"###,
    },
    Entry {
        category: HelpCategory::Instruments,
        signature: r###"device:write(parameter, value) → value"###,
        summary: [
            r###"Write a parameter through output arbitration."###,
            r###"Записать параметр через службу управления выходами."###,
        ],
        details: [
            r###"Returns the actual result. A manual actuator write takes Manual ownership; it does not pause controller computation. Pause first if intended."###,
            r###"Возвращает фактический результат. Ручная запись в выход переводит его в Manual, но не приостанавливает вычисления регулятора. При необходимости сначала вызовите pause."###,
        ],
        example: r###"return plant:write("heater_power", 10)"###,
    },
    Entry {
        category: HelpCategory::Controllers,
        signature: r###"device:pid(parameter, options) → loop"###,
        summary: [
            r###"Create a running PID controller."###,
            r###"Создать работающий PID-регулятор."###,
        ],
        details: [
            r###"Required: name, input, setpoint, kp, output_min, output_max. ki/kd default 0; gains ≥0. Configure safe_output explicitly. input must exist."###,
            r###"Обязательны name, input, setpoint, kp, output_min, output_max. ki/kd по умолчанию 0; коэффициенты ≥0. Задайте safe_output явно. input должен существовать."###,
        ],
        example: r###"loop = plant:pid("heater_power", {
    name = "heater", input = "temperature", setpoint = 80,
    kp = 1, ki = 0.02, output_min = 0, output_max = 100, safe_output = 0,
})"###,
    },
    Entry {
        category: HelpCategory::Controllers,
        signature: r###"device:on_off(parameter, options) → loop"###,
        summary: [
            r###"Create a hysteretic two-state controller."###,
            r###"Создать двухпозиционный регулятор с гистерезисом."###,
        ],
        details: [
            r###"Required: name, input, setpoint, hysteresis ≥0, output_off, output_on. Starts inactive. Only one loop can own a physical output."###,
            r###"Обязательны name, input, setpoint, hysteresis ≥0, output_off, output_on. Начальное состояние выключено. Одним физическим выходом владеет только один регулятор."###,
        ],
        example: r###"loop = plant:on_off("heater_power", {
    name = "heater", input = "temperature", setpoint = 80,
    hysteresis = 2, output_off = 0, output_on = 60, safe_output = 0,
})"###,
    },
    Entry {
        category: HelpCategory::Controllers,
        signature: r###"device:furnace(parameter, options) → loop"###,
        summary: [
            r###"Create a model-assisted thermal controller."###,
            r###"Создать тепловой регулятор с модельной компенсацией."###,
        ],
        details: [
            r###"PID-like PI settings plus ambient_temperature (°C), max_power (W), heater_lag (s), linear_loss (W/°C), radiation_loss_1000c (W). ki defaults 0; other shown fields except safe_output are required."###,
            r###"Настройки PI и ambient_temperature (°C), max_power (Вт), heater_lag (с), linear_loss (Вт/°C), radiation_loss_1000c (Вт). ki по умолчанию 0; остальные показанные поля, кроме safe_output, обязательны."###,
        ],
        example: r###"loop = plant:furnace("heater_power", {
    name = "heater", input = "temperature", setpoint = 80,
    kp = 1, ki = 0.02, output_min = 0, output_max = 100,
    ambient_temperature = 20, max_power = 2500, heater_lag = 90,
    linear_loss = 0.35, radiation_loss_1000c = 1200, safe_output = 0,
})"###,
    },
    Entry {
        category: HelpCategory::Controllers,
        signature: r###"loop:name() → string"###,
        summary: [
            r###"Return the controller's registered name."###,
            r###"Получить имя регулятора."###,
        ],
        details: [
            r###"The local Lua variable name is independent of the registered name."###,
            r###"Имя переменной Lua не обязано совпадать с зарегистрированным именем."###,
        ],
        example: r###"return loop:name()"###,
    },
    Entry {
        category: HelpCategory::Controllers,
        signature: r###"loop:parameters() → descriptors"###,
        summary: [
            r###"Discover this controller's parameters."###,
            r###"Посмотреть параметры данного регулятора."###,
        ],
        details: [
            r###"Descriptors include key, value type, access and range. A reference-managed setpoint is read-only."###,
            r###"Описания содержат key, тип, доступ и диапазон. Уставка под управлением reference доступна только для чтения."###,
        ],
        example: r###"for _, parameter in ipairs(loop:parameters()) do
    app.log(parameter.key .. ": " .. parameter.access)
end"###,
    },
    Entry {
        category: HelpCategory::Controllers,
        signature: r###"loop:read(key) → value"###,
        summary: [
            r###"Read a controller parameter."###,
            r###"Прочитать параметр регулятора."###,
        ],
        details: [
            r###"Use keys from parameters(). Reading setpoint gives the current reference value when a reference is installed."###,
            r###"Используйте ключи из parameters(). При активном reference чтение setpoint возвращает текущую уставку."###,
        ],
        example: r###"return loop:read("setpoint")"###,
    },
    Entry {
        category: HelpCategory::Controllers,
        signature: r###"loop:write(key, value) → value"###,
        summary: [
            r###"Change one controller parameter."###,
            r###"Изменить один параметр регулятора."###,
        ],
        details: [
            r###"Returns the resulting value. Validation includes actuator limits. For a managed setpoint, use reference methods instead."###,
            r###"Возвращает новое значение. Проверка учитывает ограничения выхода прибора. Управляемую уставку меняйте методами reference."###,
        ],
        example: r###"return loop:write("kp", 2)"###,
    },
    Entry {
        category: HelpCategory::Controllers,
        signature: r###"loop:configure(updates)"###,
        summary: [
            r###"Apply multiple parameter changes atomically."###,
            r###"Атомарно изменить несколько параметров."###,
        ],
        details: [
            r###"updates: table of parameter keys and values. Invalid candidates change nothing. Dynamic state is retained."###,
            r###"updates: таблица ключей и значений. Некорректный набор ничего не меняет. Динамическое состояние сохраняется."###,
        ],
        example: r###"loop:configure({kp = 2, ki = 0.03})"###,
    },
    Entry {
        category: HelpCategory::Controllers,
        signature: r###"loop:set_input(series_name)"###,
        summary: [
            r###"Change the input signal."###,
            r###"Изменить входной сигнал."###,
        ],
        details: [
            r###"The series must exist. Clears derivative/rate time history without clearing the integral."###,
            r###"Серия должна существовать. Сбрасывает временную историю производной/скорости, сохраняя интеграл."###,
        ],
        example: r###"loop:set_input("temperature_smooth")"###,
    },
    Entry {
        category: HelpCategory::Controllers,
        signature: r###"loop:state() → "running" | "paused""###,
        summary: [
            r###"Inspect computation state, not physical ownership."###,
            r###"Посмотреть состояние вычислений, а не владения выходом."###,
        ],
        details: [
            r###"A running controller can have Manual output ownership after a manual write."###,
            r###"Работающий регулятор может иметь выход в Manual после ручной записи."###,
        ],
        example: r###"return loop:state()"###,
    },
    Entry {
        category: HelpCategory::Controllers,
        signature: r###"loop:pause()"###,
        summary: [
            r###"Write safe output and pause computation."###,
            r###"Записать безопасное значение и приостановить вычисления."###,
        ],
        details: [
            r###"Waits for the safe write; also pauses when the write fails. safe_output has no implicit default. A failure is not physical safety."###,
            r###"Ждёт безопасную запись; при её ошибке также приостанавливает вычисления. У safe_output нет неявного значения. Ошибка не означает физическую безопасность."###,
        ],
        example: r###"loop:pause()"###,
    },
    Entry {
        category: HelpCategory::Controllers,
        signature: r###"loop:resume()"###,
        summary: [
            r###"Request automatic output control and resume."###,
            r###"Запросить автоматическое управление и продолжить работу."###,
        ],
        details: [
            r###"Preserves integral/ramp progress and resets time history. Manual → AutomaticPending → Automatic only after a successful automatic write."###,
            r###"Сохраняет интеграл и продвижение рампы, сбрасывает временную историю. Manual → AutomaticPending → Automatic только после успешной автоматической записи."###,
        ],
        example: r###"loop:resume()"###,
    },
    Entry {
        category: HelpCategory::Controllers,
        signature: r###"loop:reset_integral()"###,
        summary: [
            r###"Clear PID/Furnace integral only."###,
            r###"Сбросить только интеграл PID/Furnace."###,
        ],
        details: [
            r###"Preserves measurement history. Unsupported by on/off; raises an error there."###,
            r###"Сохраняет историю измерений. Не поддерживается on/off и вызовет ошибку для него."###,
        ],
        example: r###"loop:reset_integral()"###,
    },
    Entry {
        category: HelpCategory::Controllers,
        signature: r###"loop:reset()"###,
        summary: [
            r###"Reset algorithm state and restart the reference."###,
            r###"Сбросить состояние алгоритма и перезапустить reference."###,
        ],
        details: [
            r###"Retains configuration and running/paused state. This is not a safe-output write."###,
            r###"Сохраняет настройки и состояние running/paused. Не записывает безопасное значение в выход."###,
        ],
        example: r###"loop:reset()"###,
    },
    Entry {
        category: HelpCategory::Controllers,
        signature: r###"loop:remove()"###,
        summary: [
            r###"Safely remove a controller; keep diagnostic history."###,
            r###"Безопасно удалить регулятор, сохранив историю диагностик."###,
        ],
        details: [
            r###"Waits for safe pause before releasing output ownership. Failure leaves it registered for recovery. Do not reuse a removed handle."###,
            r###"Ждёт безопасную паузу перед освобождением выхода. При ошибке регулятор остаётся для восстановления. Удалённый объект больше не используйте."###,
        ],
        example: r###"loop:remove()"###,
    },
    Entry {
        category: HelpCategory::References,
        signature: r###"loop:diagnostics() → keys"###,
        summary: [
            r###"List supported diagnostic keys."###,
            r###"Посмотреть ключи диагностик."###,
        ],
        details: [
            r###"PID: setpoint, proportional, integral, derivative, output, unconstrained_output. On/off: setpoint, output. Furnace replaces derivative with feed_forward, predicted_measurement, measurement_rate."###,
            r###"PID: setpoint, proportional, integral, derivative, output, unconstrained_output. On/off: setpoint, output. Furnace вместо derivative имеет feed_forward, predicted_measurement, measurement_rate."###,
        ],
        example: r###"return loop:diagnostics()"###,
    },
    Entry {
        category: HelpCategory::References,
        signature: r###"loop:add(diagnostic, options?)"###,
        summary: [
            r###"Plot a controller diagnostic."###,
            r###"Построить график диагностики регулятора."###,
        ],
        details: [
            r###"This does not install the loop: constructors already do. options: name string or {name, color, visible, pane}; no interval. output is requested, not confirmed hardware output."###,
            r###"Не создаёт регулятор: это уже сделал конструктор. options: имя или {name, color, visible, pane}; без interval. output — расчётное, не подтверждённое прибором значение."###,
        ],
        example: r###"loop:add("output", "requested_power")"###,
    },
    Entry {
        category: HelpCategory::References,
        signature: r###"loop:reference_kind() → "fixed" | "ramp" | nil"###,
        summary: [
            r###"Inspect reference mode."###,
            r###"Посмотреть режим уставки."###,
        ],
        details: [
            r###"nil means direct setpoint control. There is no method to remove an installed reference."###,
            r###"nil означает прямое управление setpoint. Метода удаления установленного reference нет."###,
        ],
        example: r###"return loop:reference_kind()"###,
    },
    Entry {
        category: HelpCategory::References,
        signature: r###"loop:reference_parameters() → descriptors"###,
        summary: [
            r###"List reference parameter descriptors."###,
            r###"Посмотреть параметры reference."###,
        ],
        details: [
            r###"Empty before installation. Fixed has value; ramp has start, target, rate."###,
            r###"До установки массив пуст. Fixed имеет value; ramp — start, target, rate."###,
        ],
        example: r###"return loop:reference_parameters()"###,
    },
    Entry {
        category: HelpCategory::References,
        signature: r###"loop:set_fixed_reference(value)"###,
        summary: [
            r###"Install a constant managed setpoint."###,
            r###"Установить постоянную управляемую уставку."###,
        ],
        details: [
            r###"value is a finite number in input engineering units. Controller setpoint becomes read-only."###,
            r###"value — конечное число в единицах входа. Параметр регулятора setpoint становится доступным только для чтения."###,
        ],
        example: r###"loop:set_fixed_reference(80)"###,
    },
    Entry {
        category: HelpCategory::References,
        signature: r###"loop:set_ramp_reference(options)"###,
        summary: [
            r###"Install a linear setpoint ramp."###,
            r###"Установить линейную рампу уставки."###,
        ],
        details: [
            r###"Required start, target and positive rate in units/second. Direction is inferred. First input sample establishes time; pause freezes progress."###,
            r###"Обязательны start, target и положительная rate в единицах/с. Направление определяется автоматически. Первый отсчёт задаёт начало времени; пауза замораживает продвижение."###,
        ],
        example: r###"loop:set_ramp_reference({start = 20, target = 80, rate = 0.5})"###,
    },
    Entry {
        category: HelpCategory::References,
        signature: r###"loop:read_reference(key) → value"###,
        summary: [
            r###"Read a reference parameter."###,
            r###"Прочитать параметр reference."###,
        ],
        details: [
            r###"A reference must be installed. Read loop:read("setpoint") for the current generated value."###,
            r###"Reference должен быть установлен. Текущую генерируемую уставку читайте через loop:read("setpoint")."###,
        ],
        example: r###"return loop:read_reference("target")"###,
    },
    Entry {
        category: HelpCategory::References,
        signature: r###"loop:write_reference(key, value) → value"###,
        summary: [
            r###"Change one reference parameter."###,
            r###"Изменить один параметр reference."###,
        ],
        details: [
            r###"Changing ramp target/rate preserves progress; writing start restarts it. Fixed uses the key value."###,
            r###"Изменение target/rate сохраняет продвижение рампы; запись start перезапускает её. Для fixed используйте ключ value."###,
        ],
        example: r###"return loop:write_reference("target", 100)"###,
    },
    Entry {
        category: HelpCategory::References,
        signature: r###"loop:configure_reference(updates)"###,
        summary: [
            r###"Apply reference changes atomically."###,
            r###"Атомарно изменить reference."###,
        ],
        details: [
            r###"Use keys from reference_parameters(). Invalid combinations leave the reference unchanged."###,
            r###"Используйте ключи из reference_parameters(). Некорректный набор не меняет reference."###,
        ],
        example: r###"loop:configure_reference({target = 120, rate = 1})"###,
    },
    Entry {
        category: HelpCategory::Panels,
        signature: r###"app.register_script(script)"###,
        summary: [
            r###"Retain callbacks and install declared panels."###,
            r###"Сохранить функции и создать объявленные панели."###,
        ],
        details: [
            r###"script requires id; panels is optional. Callbacks are named fields, without implicit self. Re-registering the ID replaces its table/panels."###,
            r###"script требует id; panels необязателен. Обработчики — именованные поля, без неявного self. Повторная регистрация ID заменяет таблицу и панели."###,
        ],
        example: r###"app.register_script({id = "panel",
    refresh = function() app.log("Refresh") end,
    panels = {{id = "main", title = "Experiment", controls = {
        {kind = "readout", id = "state", label = "State", initial = "Ready"},
        {kind = "button", id = "refresh", label = "Refresh", on_click = "refresh"},
    }}},
})"###,
    },
    Entry {
        category: HelpCategory::Panels,
        signature: r###"readout / number / toggle / button"###,
        summary: [
            r###"Declare one of four panel widget kinds."###,
            r###"Объявить один из четырёх видов элементов панели."###,
        ],
        details: [
            r###"Every widget requires kind, id and label. readout: string initial; number: finite initial/min/max, positive step, on_change; toggle: boolean initial, on_change; button: on_click."###,
            r###"Каждый элемент требует kind, id и label. readout: строка initial; number: конечные initial/min/max, положительный step, on_change; toggle: boolean initial, on_change; button: on_click."###,
        ],
        example: r###"local controls = {
    {kind = "readout", id = "state", label = "State", initial = "Ready"},
    {kind = "number", id = "target", label = "Target", initial = 80,
        min = 0, max = 200, step = 1, on_change = "set_target"},
    {kind = "toggle", id = "enabled", label = "Enabled", on_change = "set_enabled"},
    {kind = "button", id = "refresh", label = "Refresh", on_click = "refresh"},
}"###,
    },
    Entry {
        category: HelpCategory::Panels,
        signature: r###"app.set_control(script, panel, control, value)"###,
        summary: [
            r###"Update a registered widget value."###,
            r###"Обновить значение зарегистрированного элемента."###,
        ],
        details: [
            r###"Use stable IDs. Value type must match readout/number/toggle; buttons have no value. Does not call the widget callback."###,
            r###"Передавайте стабильные ID. Тип должен соответствовать readout/number/toggle; у кнопок значения нет. Обработчик элемента не вызывается."###,
        ],
        example: r###"app.set_control("panel", "main", "state", "Ready")"###,
    },
    Entry {
        category: HelpCategory::Panels,
        signature: r###"app.set_control_enabled(script, panel, control, enabled, reason?)"###,
        summary: [
            r###"Enable/disable user interaction."###,
            r###"Разрешить или запретить взаимодействие с элементом."###,
        ],
        details: [
            r###"enabled: boolean; optional reason is shown when disabled. This is a UI guard, not a hardware interlock."###,
            r###"enabled: boolean; необязательная reason отображается при блокировке. Это ограничение интерфейса, не аппаратная защита."###,
        ],
        example: r###"app.set_control_enabled("panel", "main", "refresh", false, "Busy")"###,
    },
    Entry {
        category: HelpCategory::Panels,
        signature: r###"app.unregister_script(id)"###,
        summary: [
            r###"Remove script callbacks and panels."###,
            r###"Удалить обработчики и панели скрипта."###,
        ],
        details: [
            r###"Does not remove instruments/controllers created by the script. Stop those explicitly if needed."###,
            r###"Не удаляет приборы и регуляторы, созданные скриптом. При необходимости остановите их явно."###,
        ],
        example: r###"app.unregister_script("panel")"###,
    },
    Entry {
        category: HelpCategory::Scenarios,
        signature: r###"app.scenario({id}) → scenario"###,
        summary: [
            r###"Create a named automation run."###,
            r###"Создать именованный сценарий автоматизации."###,
        ],
        details: [
            r###"Reusing an ID cancels the old run. Callbacks resolve to global functions first, otherwise unique registered script functions."###,
            r###"Повторное использование ID отменяет старый запуск. Обработчик ищется сначала среди глобальных функций, затем среди однозначных функций зарегистрированных скриптов."###,
        ],
        example: r###"function report(event) app.log(event.trigger) end
scenario = app.scenario({id = "experiment"})"###,
    },
    Entry {
        category: HelpCategory::Scenarios,
        signature: r###"scenario:id() → string"###,
        summary: [
            r###"Return the scenario ID."###,
            r###"Получить ID сценария."###,
        ],
        details: [
            r###"The stable ID identifies the scenario; runtime generation IDs guard late action completions."###,
            r###"Стабильный ID обозначает сценарий; внутренние ID запусков защищают от запоздалых завершений действий."###,
        ],
        example: r###"return scenario:id()"###,
    },
    Entry {
        category: HelpCategory::Scenarios,
        signature: r###"scenario:after(seconds, callback)"###,
        summary: [
            r###"Schedule a one-shot relative timer."###,
            r###"Запланировать одноразовый относительный таймер."###,
        ],
        details: [
            r###"seconds: finite and ≥0; monotonic clock. callback: function name string; receives an event table."###,
            r###"seconds: конечное число ≥0; монотонные часы. callback: имя функции строкой; она получает таблицу события."###,
        ],
        example: r###"scenario:after(5, "report")"###,
    },
    Entry {
        category: HelpCategory::Scenarios,
        signature: r###"scenario:at(unix_timestamp, callback)"###,
        summary: [
            r###"Schedule a one-shot absolute timer."###,
            r###"Запланировать одноразовый абсолютный таймер."###,
        ],
        details: [
            r###"Unix time in seconds. callback receives event.trigger = "absolute_time"."###,
            r###"Unix-время в секундах. Обработчик получает event.trigger = "absolute_time"."###,
        ],
        example: r###"scenario:at(os.time() + 60, "report")"###,
    },
    Entry {
        category: HelpCategory::Scenarios,
        signature: r###"scenario:when(condition, callback)"###,
        summary: [
            r###"Wait for a measurement condition."###,
            r###"Дождаться условия по измерениям."###,
        ],
        details: [
            r###"One operator: above, below, inside, outside, stable, rate_above, rate_below, stale_for_seconds, all or any. Optional hold/hysteresis; only rising edge. See docs/scenarios.md for combinations."###,
            r###"Один оператор: above, below, inside, outside, stable, rate_above, rate_below, stale_for_seconds, all или any. Возможны выдержка/гистерезис; только rising. Сочетания описаны в docs/scenarios.md."###,
        ],
        example: r###"scenario:when({series = "temperature", above = 80, for_seconds = 5}, "report")"###,
    },
    Entry {
        category: HelpCategory::Scenarios,
        signature: r###"scenario:race(alternatives)"###,
        summary: [
            r###"Run only the first matching alternative."###,
            r###"Выполнить только первое сработавшее условие."###,
        ],
        details: [
            r###"Each alternative has after/at/when and callback. Losers are cancelled; simultaneously ready alternatives use declaration order."###,
            r###"Каждая ветвь содержит after/at/when и callback. Остальные отменяются; при одновременной готовности важен порядок объявления."###,
        ],
        example: r###"scenario:race({
    {when = {series = "temperature", above = 80}, callback = "report"},
    {after = 60, callback = "report"},
})"###,
    },
    Entry {
        category: HelpCategory::Scenarios,
        signature: r###"scenario:stage(name, definition)"###,
        summary: [
            r###"Declare a stage and transitions."###,
            r###"Объявить стадию и переходы."###,
        ],
        details: [
            r###"Required enter callback. transitions contain after/at/when, next and optional reason. Declare all targets before start."###,
            r###"Обязателен обработчик enter. transitions содержат after/at/when, next и необязательную reason. Объявите все цели до start."###,
        ],
        example: r###"scenario:stage("heating", {enter = "report",
    transitions = {{after = 10, next = "done"}},
})
scenario:stage("done", {enter = "report"})"###,
    },
    Entry {
        category: HelpCategory::Scenarios,
        signature: r###"scenario:start(stage)"###,
        summary: [
            r###"Start the stage graph once."###,
            r###"Однократно запустить граф стадий."###,
        ],
        details: [
            r###"Transitions activate after the enter callback and its tracked actions complete. Stages cannot be added after start."###,
            r###"Переходы активируются после завершения enter и его отслеживаемых действий. После start добавлять стадии нельзя."###,
        ],
        example: r###"scenario:start("heating")"###,
    },
    Entry {
        category: HelpCategory::Scenarios,
        signature: r###"scenario:on_stop(callback)"###,
        summary: [
            r###"Set cleanup for completion or normal stop."###,
            r###"Задать завершение для complete или обычного stop."###,
        ],
        details: [
            r###"callback receives status/reason and may enqueue tracked actions. Registered once per run."###,
            r###"Обработчик получает status/reason и может отправлять отслеживаемые действия. Регистрируется один раз на запуск."###,
        ],
        example: r###"scenario:on_stop("report")"###,
    },
    Entry {
        category: HelpCategory::Scenarios,
        signature: r###"scenario:on_error(callback)"###,
        summary: [
            r###"Set cleanup for callback/action failure."###,
            r###"Задать завершение при ошибке обработчика или действия."###,
        ],
        details: [
            r###"callback receives the original error. Cleanup failures are reported and do not recursively invoke cleanup."###,
            r###"Обработчик получает исходную ошибку. Ошибки завершения отображаются, но не вызывают его рекурсивно."###,
        ],
        example: r###"scenario:on_error("report")"###,
    },
    Entry {
        category: HelpCategory::Scenarios,
        signature: r###"scenario:complete(reason)"###,
        summary: [
            r###"Finish successfully after actions and cleanup."###,
            r###"Успешно завершить после действий и обработчика завершения."###,
        ],
        details: [
            r###"reason: nonempty string. Pending triggers are cleared; active tracked actions settle before finalization."###,
            r###"reason: непустая строка. Ожидающие условия отменяются; активные отслеживаемые действия завершаются до финализации."###,
        ],
        example: r###"scenario:complete("Target reached")"###,
    },
    Entry {
        category: HelpCategory::Scenarios,
        signature: r###"scenario:stop(reason)"###,
        summary: [
            r###"Stop normally with cleanup."###,
            r###"Остановить штатно с обработчиком завершения."###,
        ],
        details: [
            r###"reason: nonempty string. Uses on_stop; does not automatically pause arbitrary controllers."###,
            r###"reason: непустая строка. Вызывает on_stop; произвольные регуляторы автоматически не приостанавливаются."###,
        ],
        example: r###"scenario:stop("Operator stopped experiment")"###,
    },
    Entry {
        category: HelpCategory::Scenarios,
        signature: r###"scenario:cancel()"###,
        summary: [
            r###"Cancel immediately without cleanup callbacks."###,
            r###"Немедленно отменить без обработчиков завершения."###,
        ],
        details: [
            r###"Cancels pending tasks; cannot retract already dispatched hardware writes. Not a safe-output command."###,
            r###"Отменяет ожидающие задачи, но не отзывает уже отправленные записи в прибор. Не является командой безопасного выхода."###,
        ],
        example: r###"scenario:cancel()"###,
    },
    Entry {
        category: HelpCategory::Setup,
        signature: r###"setup = function() ... end"###,
        summary: [
            r###"Initialize the experiment from a profile."###,
            r###"Инициализировать эксперимент из профиля."###,
        ],
        details: [
            r###"Profile top level is evaluated before app exists. Put actions in setup; scripts run afterward in order. Relative resources follow the profile directory."###,
            r###"Верхний уровень профиля вычисляется до появления app. Действия помещайте в setup; затем по порядку выполняются scripts. Относительные пути считаются от каталога профиля."###,
        ],
        example: r###"return {
    emulator = {transport = "memory", script = "emulator_scripts/furnace_plant.lua"},
    setup = function() app.start_emu() end,
}"###,
    },
    Entry {
        category: HelpCategory::Setup,
        signature: r###"instruments / read(id, key, time) / write(id, key, value, time)"###,
        summary: [
            r###"Define a model in the separate emulator Lua state."###,
            r###"Определить модель в отдельной Lua-среде эмулятора."###,
        ],
        details: [
            r###"Catalog positions supply one-based IDs. time is elapsed seconds, not Unix time. A write returns actual stored value. No app table exists here."###,
            r###"Позиции в каталоге задают ID с 1. time — прошедшие секунды, не Unix-время. write возвращает сохранённое значение. Таблицы app здесь нет."###,
        ],
        example: r###"local value = 20
instruments = {{name = "Example", parameters = {
    {key = "value", type = "number", access = "read_write", series = true},
}}}
function read(id, key, time) return value end
function write(id, key, requested, time) value = requested; return value end"###,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_examples_compile_and_translations_are_complete() {
        let lua = mlua::Lua::new();
        let mut signatures = std::collections::HashSet::new();
        for entry in ENTRIES {
            assert!(
                signatures.insert(entry.signature),
                "duplicate {}",
                entry.signature
            );
            assert!(!entry.summary(HelpLanguage::English).is_empty());
            assert!(!entry.summary(HelpLanguage::Russian).is_empty());
            assert!(!entry.details(HelpLanguage::English).is_empty());
            assert!(!entry.details(HelpLanguage::Russian).is_empty());
            lua.load(entry.example)
                .into_function()
                .unwrap_or_else(|error| panic!("{}: {error}", entry.signature));
        }
        for category in HelpCategory::ALL
            .iter()
            .copied()
            .filter(|category| *category != HelpCategory::All)
        {
            assert!(ENTRIES.iter().any(|entry| entry.category == category));
        }
    }

    #[test]
    fn search_handles_case_whitespace_both_languages_and_categories() {
        let furnace = ENTRIES
            .iter()
            .find(|entry| entry.signature.starts_with("device:furnace("))
            .unwrap();
        assert!(furnace.matches(HelpCategory::All, "  FURNACE   heater_lag "));
        assert!(furnace.matches(HelpCategory::Controllers, "тепловой"));
        assert!(!furnace.matches(HelpCategory::Panels, "furnace"));
        assert!(!furnace.matches(HelpCategory::All, "nonexistent_function"));
        assert!(
            ENTRIES
                .iter()
                .all(|entry| entry.matches(HelpCategory::All, "   "))
        );
    }

    #[test]
    fn queued_help_examples_validate_against_real_bindings() {
        let lua = mlua::Lua::new();
        let (commands, _receiver) = crossbeam_channel::unbounded();
        let (events, _events_receiver) = crossbeam_channel::unbounded();
        let definition = crate::lua_application_definition::apply_lua_definition(
            "return {connections = {primary = {port = 'COM248'}}}",
            &Default::default(),
        )
        .unwrap();
        crate::lua_api::install(&lua, commands, events, &definition).unwrap();
        for entry in ENTRIES.iter().filter(|entry| {
            matches!(
                entry.category,
                HelpCategory::Application | HelpCategory::Series | HelpCategory::Panels
            )
        }) {
            lua.load(entry.example)
                .exec()
                .unwrap_or_else(|error| panic!("{}: {error}", entry.signature));
        }
    }

    #[test]
    fn help_covers_every_registered_application_function_and_userdata_method() {
        let lua = mlua::Lua::new();
        let (commands, _receiver) = crossbeam_channel::unbounded();
        let (events, _events_receiver) = crossbeam_channel::unbounded();
        crate::lua_api::install(&lua, commands, events, &Default::default()).unwrap();
        let app: mlua::Table = lua.globals().get("app").unwrap();
        for pair in app.pairs::<String, mlua::Value>() {
            let (name, _) = pair.unwrap();
            assert!(
                ENTRIES
                    .iter()
                    .any(|entry| entry.signature.starts_with(&format!("app.{name}("))),
                "missing app.{name}"
            );
        }
        for (source, prefix) in [
            (include_str!("../../lua_api/controllers.rs"), "loop"),
            (include_str!("../../lua_api/metakon.rs"), "device"),
            (
                include_str!("../../lua_api/virtual_instrument.rs"),
                "device",
            ),
            (include_str!("../../lua_api/scenarios.rs"), "scenario"),
        ] {
            for call in source.split("methods.add_method").skip(1) {
                let name = call.split('"').nth(1).unwrap();
                assert!(
                    ENTRIES
                        .iter()
                        .any(|entry| entry.signature.starts_with(&format!("{prefix}:{name}("))),
                    "missing {prefix}:{name}"
                );
            }
        }
    }
}
