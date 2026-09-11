# Постоянный контекст и правила работы

`com_port_reader` — desktop-приложение на Rust 2024 для лабораторной автоматизации и длительной регистрации процессов под Windows 10. Основной стек: `eframe/egui`, `egui_plot`, `crossbeam-channel`, `serialport`, `mlua` Lua 5.4 и `rusqlite`.

Работать небольшими шагами: **один логический шаг — один git commit**. После каждого законченного шага выполнять:

```powershell
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
git diff --check
```

Перед изменениями изучать существующий код, не дублировать механизмы и сохранять public/Lua API contracts без конкретной причины их менять. Не делать крупных рефакторингов и не строить framework ради framework: архитектурный недостаток, обнаруженный новой функцией, исправлять отдельным локальным refactor-коммитом. Сохранять сложившийся Rust enum/static dispatch; Lua писать компактно и идиоматично.

Критичная цепочка управления:

```text
Series
  ↓
Signal processing / filters
  ↓
ControlLoop
  ↓
Controller enum (PID / OnOff / Furnace)
  ↓
OutputControl / arbiter
  ↓
Instrument output
```

Safety-инварианты нельзя обходить:

- обычная manual-запись в контролируемый параметр переводит output в `Manual`;
- `controller:pause()` применяет `safe_output`, переводит output в `Manual`, затем ставит processing controller в `Paused`;
- `controller:resume()` проходит через `AutomaticPending`, переводит output в `Automatic` только после успешной записи и выполняет rollback при ошибке;
- отключение GUI-контрола — только UX-защита; критичные ограничения по-прежнему проверяются в `process_control`/`output_control`;
- при изменениях presentation, control panels и scenarios не менять без необходимости semantics acquisition, recording, references и output arbitration.

Дальняя цель проекта — работа с реальными лабораторными приборами, в перспективе Modbus RTU, около 10 сигналов с polling порядка 1 Hz, процессы до двух недель, Lua procedures и воспроизводимый протокол эксперимента. Параметры furnace model (`linear_loss`, `radiation_loss_1000c`, `heater_lag`) предполагается определять по manual runs; `max_power` желательно брать из измеренной электрической мощности.

Архитектура сейчас выглядит здоровой. Большой рефакторинг уже дал результат: сервисы разделены, команды проходят через единый runtime, Lua изолирован в своём потоке, а безопасность управления сосредоточена в `output_control` и `process_control`. Ещё один общий «рефакторинг ради чистоты» делать не следует.

## Статус выполнения на 2026-09-11

Завершены и сохранены отдельными коммитами:

1. `1f32b27 feat: add declarative plot pane layout` — стабильные `PlotPaneKey`, декларативный `plot_panes`, совместимый default layout.
2. `2e79c3c feat: assign new series to plot panes` — общий `SeriesPresentation { color, visible, pane }` для serial/instrument series, filters и controller diagnostics; проверка неизвестного `pane`.
3. `753e363 feat: move series between plot panes` — `app.set_series_pane`, единое хранение назначения в `SeriesStore`, сохранение при rename и очистка назначения при удалении панели.
4. `3d29d1a refactor: separate control state properties` — общие поля `ControlState` отделены от `Readout/Number/Toggle/Button`, `pending` остаётся независимым.
5. `c3d112f feat: control panel enabled state from Lua` — immutable registry валидированных control metadata и `app.set_control_enabled(..., reason)`.
6. `254b88a feat: publish application runtime events` — отдельный `ApplicationEventHub` для измерений и lifecycle действий `Requested/Applied/Failed`, независимый от SQLite/timeline.
7. `f89a33c feat: add event-driven Lua scenarios` — `ScenarioService`, относительные таймеры, пороговые условия и короткие Lua callbacks.
8. `97f5d21 feat: sequence scenario actions by outcome` — команды callback связаны со сценарием; следующий callback ждёт `Applied`, а `Failed` останавливает сценарий.
9. `012746c feat: schedule scenarios at absolute time` — добавлен `scenario:at(unix_timestamp, callback)` с абсолютным временем UTC.
10. `a62b0d1 test: cover measurement-driven Lua scenario` — сквозной тест `ProcessRecorder → ApplicationEvent::Measurements → ScenarioService → Lua callback → ActionApplied`.

В сценарном коммите реализовано:

- `ScenarioService`, `app.scenario({ id = ... })`, `scenario:after(...)`, `scenario:when(...)` и `scenario:cancel()`;
- относительные таймеры и выдержки используют `Instant`;
- пороговые условия поддерживают `above`/`below`, `for_seconds`, `hysteresis`, `edge = "rising"`;
- Rust вызывает только короткий Lua callback; callback может быть global function либо однозначно найден в зарегистрированном application script;
- ошибка callback останавливает сценарий;
- смена runtime/profile уничтожает сервис и отменяет его задания;
- запуск, callback, остановка и ошибки пишутся через `LogHandle`, то есть попадают в process log;
- добавлены unit tests для Lua API, порогов/выдержки/гистерезиса и выполнения callback в Lua worker.

Все пункты этого этапа реализованы. `lua_types/app.d.lua`, встроенная справка на английском и русском и примеры обновлены коммитом, содержащим эту запись. На текущем `HEAD` проходят debug `fmt/test/clippy/diff --check`, `cargo build --release` и все release-тесты (672 теста). Незавершённых изменений функциональности больше нет.

Текущая архитектурная граница: декларативная раскладка находится в `presentation`/`application_definition`, команды входят через `ApplicationRuntime`, события измерений и результатов действий выходят через `ApplicationEventHub`, а `ScenarioService` не опрашивает GUI и не выполняет длинный Lua-код. Дополнительный общий рефакторинг сейчас не требуется.

## Исходный план (выполнен)

### 1. Панели графиков из Lua

Многопанельность уже реализована: есть панели, их размеры и привязка `SeriesId → PlotPaneId`. Но `PlotPaneId` сейчас внутренний числовой идентификатор, а Lua нужен стабильный строковый ID.

Я бы ввёл понятие `PlotPaneKey`, например `"temperature"`, `"power"`, `"pid_terms"`. Числовой `PlotPaneId` можно оставить внутренним идентификатором egui.

Начальную раскладку лучше объявлять декларативно в профиле:

```lua
return {
    plot_panes = {
        {
            id = "temperature",
            title = "Temperature",
            weight = 2.0,
        },
        {
            id = "control",
            title = "Control",
            weight = 1.0,
        },
    },

    -- application, connections, scripts...
}
```

Первая панель становится панелью по умолчанию. Если `plot_panes` отсутствует, создаётся нынешняя единственная панель — старые профили работают без изменений.

В параметры всех API, создающих серии, добавляется опциональное поле:

```lua
plant:add("temperature", {
    name = "temperature",
    interval = 0.5,
    color = "#D32F2F",
    pane = "temperature",
})
```

Это должно работать одинаково для:

- serial series;
- instrument series;
- filters;
- controller diagnostics.

Сейчас общие параметры серии уже собраны в `LuaSeriesOptions` в [series.rs](/D:/rust/com_port_reader/src/lua_api/series.rs:13), но фильтры и диагностика используют отдельные структуры. Я бы выделил общий небольшой `SeriesPresentation`:

```text
color
visible
pane
```

и прикрепил его к серии. Это хорошо согласуется с текущим устройством: `visible` и `color` уже являются частью состояния серии. Главное преимущество — `pane` путешествует вместе с командой создания серии, поэтому не возникает гонки между «серия создана» и «GUI получил отдельное событие о её размещении».

Для последующего перемещения полезно иметь:

```lua
app.set_series_pane("heater_power", "control")
```

Внутри это может быть обычная `SeriesCommand`, но её не обязательно записывать как процессное действие.

`app.add_plot(...)` я бы добавлял только если действительно нужна динамическая перестройка во время процесса. Для стартовой раскладки декларативный `plot_panes` проще, надёжнее и валидируется до запуска оборудования.

Важные правила:

- отсутствие `pane` означает первую панель;
- опечатка в явно указанном `pane` должна давать ошибку, а не молча отправлять серию на первый график;
- после удаления панели назначенные ей серии явно возвращаются на первую;
- переименование серии не теряет назначение, поэтому внутри связь должна оставаться по `SeriesId`;
- раскладку пока не стоит писать в SQLite. Это UI-конфигурация, а база сейчас описывает процесс. Если потом понадобится сохранять ручные перестановки пользователя, лучше отдельный файл состояния/настроек.

### 2. Изменяемая панель управления

Основа уже почти готова. Lua умеет изменять значения через `app.set_control`, а `pending` блокирует повторное действие до завершения callback. Это реализовано в [ControlState](/D:/rust/com_port_reader/src/components/control_panel_model.rs:172).

Перед добавлением новых свойств я бы немного изменил форму модели. Сейчас `id` и `label` повторяются в каждом варианте enum. При добавлении `enabled`, `visible`, `disabled_reason` повторение станет заметно хуже. Естественнее:

```text
ControlState
    id
    label
    enabled
    disabled_reason
    kind: Readout | Number | Toggle | Button
```

При этом `pending` и `enabled` должны оставаться разными состояниями:

```text
effective_enabled = enabled && !pending
```

Lua API лучше сделать явным и типизированным:

```lua
app.set_control_enabled(
    SCRIPT_ID,
    PANEL_ID,
    "pause_pid",
    false,
    "PID is already paused"
)
```

Позже симметрично можно добавить `set_control_visible` или `set_control_label`. Перерегистрировать всю панель ради блокировки кнопки не стоит: это может сбросить текущее значение, черновик редактора и `pending`.

Есть ещё небольшой архитектурный дефект: при `app.set_control` тип элемента повторно определяется через исходную Lua-таблицу в [registered_control_kind](/D:/rust/com_port_reader/src/lua_application_script.rs:442), тогда как GUI хранит уже провалидированный снимок определения. Lua-таблицу теоретически можно изменить после регистрации, и два представления разойдутся. Перед расширением API свойств лучше хранить отдельный неизменяемый реестр валидированных метаданных контролов.

И важное различие: отключённая кнопка — только UX-защита. Ограничение, критичное для оборудования, всё равно должно проверяться в `process_control`/`output_control`. Lua-консоль и другие команды могут обойти GUI.

### 3. Lua-сценарии процесса

Здесь потребуется отдельная подсистема. Делать сценарии через `sleep`, длинные циклы или постоянный опрос из Lua нельзя: Lua выполняется последовательно в одном worker-потоке [lua_worker.rs](/D:/rust/com_port_reader/src/lua_worker.rs:189), а вычисление ограничено 500 мс [lua_execution.rs](/D:/rust/com_port_reader/src/lua_execution.rs:5).

Правильная модель — событийная:

```text
измерения / таймеры / состояния контроллеров
                  ↓
          ScenarioService (Rust)
                  ↓
       короткий Lua callback
                  ↓
      существующие UserCommand
```

`ScenarioService` должен:

- владеть таймерами и зарегистрированными условиями;
- использовать `Instant` для относительных задержек и выдержек;
- использовать wall-clock только для абсолютного запуска;
- отслеживать пересечение порога, гистерезис и `for_seconds`;
- отправлять в Lua worker команду вроде `InvokeScenarioCallback`;
- отменять все задания при смене профиля;
- записывать запуск, переходы, остановку и ошибку сценария в процессный журнал.

Lua API может выглядеть примерно так:

```lua
local process = app.scenario({
    id = "heat_cycle",
})

process:after(10.0, "start_heating")

process:when({
    series = "temperature",
    above = 150.0,
    for_seconds = 5.0,
    edge = "rising",
}, "switch_to_hold")
```

Первую версию условий лучше сделать декларативной. Не вызывать произвольную Lua-функцию на каждом измерении: это создаст нагрузку, очереди событий и менее предсказуемое поведение. Rust определяет факт срабатывания, Lua решает, какие команды выполнить.

До сценариев желательно решить ещё две инфраструктурные вещи:

1. Единый поток событий измерений. Сейчас исходные измерения сохраняются непосредственно в worker-потоке [worker.rs](/D:/rust/com_port_reader/src/worker.rs:201), а обработанные — через [application_runtime.rs](/D:/rust/com_port_reader/src/application_runtime.rs:617). Сценарий не должен опрашивать `SeriesStore`; оба пути должны публиковать принятые измерения в общий канал.

2. Результаты асинхронных действий. Например, `app.start()` сейчас означает «команда поставлена в очередь», а не «запуск успешно завершён». Надёжный сценарий должен уметь продолжить переход только после `Applied` либо уйти в ошибку после `Failed`. Для этого нужен нормальный `ApplicationEvent/ActionResult`, а не использование `ProcessRecorder` как внутренней шины событий.

### Выполненный порядок

Я бы двигался так:

1. Ввести стабильные ID панелей и небольшую модель presentation/layout.
2. Добавить декларативные `plot_panes` и опциональный `pane` у всех видов серий.
3. Добавить `set_series_pane`.
4. Привести `ControlState` к общей структуре свойств.
5. Добавить `set_control_enabled` с причиной блокировки.
6. Ввести внутренние события измерений и результатов действий.
7. Реализовать `ScenarioService`: сначала таймеры, затем пороговые условия, потом полноценные переходы и восстановление после ошибок.

Итоговая мысль подтвердилась: архитектуру не пришлось широко переделывать. Сформированы два явных направления данных — `commands` внутрь runtime и `events` наружу; представление графиков и состояние контролов отделены от критичной логики управления оборудованием.
