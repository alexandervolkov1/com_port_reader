Проект `com_port_reader` функционально уже доведён до текущего запланированного состояния, включая предыдущие architecture cleanup, source organization и release hardening.

Теперь нужен **отдельный структурный refactoring pass без добавления новой функциональности**.

Главное правило:

```text
1 логический рефакторинг = 1 commit
```

После каждого шага:

```powershell
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
git diff --check
```

Перед началом обязательно:

```powershell
git status
git log --oneline -15
```

Изучи текущее состояние репозитория и не повторяй уже сделанные изменения.

## Общие ограничения

Этот этап не должен менять observable behavior.

Не менять без отдельной причины:

* Lua API;
* имена Lua parameters;
* profile format;
* controller semantics;
* output arbitration;
* safe output;
* pause/resume semantics;
* acquisition scheduling;
* process recording;
* serial protocol behavior;
* error semantics.

Не создавать abstraction/framework только ради уменьшения количества строк.

Сначала определять ответственность модуля, потом переносить код.

Не объединять structural refactor и behavioral changes в одном commit.

---

## 1. Разделить `lua_api.rs`

Это главный кандидат.

Сейчас `lua_api.rs` знает слишком много сразу:

```text
application command registration
series API
filter API
controller creation/API
virtual instrument API
Metakon API
Lua userdata wrappers
Lua/Rust value conversion
option parsing and validation
```

Цель:

```text
src/lua_api/
    mod.rs
    series.rs
    filters.rs
    controllers.rs
    virtual_instrument.rs
    metakon.rs
    conversion.rs
```

Точные имена модулей выбрать после просмотра зависимостей.

`lua_api/mod.rs` должен оставаться небольшим фасадом с примерно такой ответственностью:

```text
install Lua API
assemble submodules
shared small helpers/types
```

Особенно желательно вынести:

* `LuaControllerHandle`;
* controller creation helpers;
* PID/on-off constructors;
* virtual instrument userdata;
* Metakon userdata;
* parameter/value conversion;
* series/filter option parsing.

Не менять Lua-visible names и signatures.

Предлагаемый commit:

```text
refactor: split Lua API into focused modules
```

---

## 2. Привести controller Lua bindings к общей структуре

После разделения `lua_api.rs` посмотреть, насколько PID, OnOff и будущие controller bindings используют одинаковую инфраструктуру.

Цель не в том, чтобы создать generic framework любой ценой.

Нужно оставить общий код только там, где реально одинаковы:

```text
controller handle
read/write/configure
parameters
diagnostics
pause/resume/reset
reference management
```

Создание конкретных controller types должно оставаться рядом с соответствующим Lua constructor/parser.

Не смешивать этот шаг с изменением controller algorithms.

Commit только если получается самостоятельное улучшение:

```text
refactor: simplify Lua controller bindings
```

Если после шага 1 код уже чистый, этот commit не нужен.

---

## 3. Разделить `signal_processing/service.rs`

Изучить текущий `ProcessingService`.

Он совмещает:

```text
command definitions
ProcessingHandle
service state
service thread/runtime loop
filter operations
controller operations
diagnostic bindings
errors
```

Разделить только естественные ответственности.

Возможная структура:

```text
signal_processing/
    filter.rs
    graph.rs

    service/
        mod.rs
        command.rs
        handle.rs
        runtime.rs
        error.rs
```

или более компактный вариант, если отдельных файлов получается слишком много.

Главное сохранить:

```text
one owner of processing state
same command ordering
same channel behavior
same controller lifecycle
same filtering behavior
```

Особенно не менять concurrency semantics просто в рамках cleanup.

Commit:

```text
refactor: split signal processing service
```

---

## 4. Проверить `output_control/service.rs`

Не рефакторить его автоматически только потому, что файл большой.

Сначала определить, есть ли там действительно независимые ответственности.

Наиболее вероятные кандидаты:

```text
request/command types
OutputHandle
pending write tracking
runtime/service loop
write completion handling
errors
```

Если разделение делает ownership и safety flow яснее, выполнить.

При этом обязательно сохранить текущие инварианты:

```text
Manual
AutomaticPending
Automatic

safe output
controller ownership
tracked writes
rollback/error behavior
```

Safety-critical код не упрощать ценой изменения порядка операций.

Возможный commit:

```text
refactor: separate output control runtime concerns
```

Если модуль после анализа уже логически цельный, оставить его как есть.

---

## 5. Проверить `application_runtime.rs`

Здесь уже существуют отдельные command-handler modules, поэтому сначала убедиться, что остаток `application_runtime.rs` действительно стоит дробить.

Не надо создавать дополнительные файлы только ради line count.

Возможные независимые обязанности:

```text
runtime construction/startup
shutdown
profile/Lua initialization
worker lifecycle
process action recording
event dispatch
```

Если orchestration читается последовательно и понятно, оставить его.

Если часть логики явно является отдельным subsystem, вынести её.

Commit только при реальной пользе:

```text
refactor: simplify application runtime orchestration
```

---

## 6. Проверить `process_control`, но не переделывать заново

Parameter ownership уже перенесён в конкретные controller implementations.

Сохранить принцип:

```text
pid.rs
    owns PID parameter knowledge

on_off.rs
    owns OnOff parameter knowledge

furnace.rs
    owns Furnace parameter knowledge

controller.rs
    generic enum dispatch and common API
```

Не возвращать controller-specific parameter logic обратно в `controller.rs`.

Посмотреть только, можно ли уменьшить `controller.rs` естественным выносом:

```text
ControllerOutput
common controller errors
diagnostic dispatch
```

Но делать это только если получаются самостоятельные понятные сущности.

Не дробить файл на `controller_output.rs`, `controller_error.rs` и ещё семь файлов просто ради количества файлов.

---

## 7. Help/documentation code

`components/help_view.rs` можно разделить по тематическим разделам, если он мешает навигации.

Например:

```text
components/help/
    mod.rs
    application.rs
    lua.rs
    instruments.rs
    controllers.rs
```

Это низкорисковый refactor, поскольку help является в основном presentation code.

Но он имеет меньший приоритет, чем `lua_api` и processing services.

Commit:

```text
refactor: split help content into sections
```

---

## 8. Не переносить unit tests без причины

Многие Rust modules выглядят большими из-за большого количества inline unit tests.

Это нормально.

Не переносить автоматически все:

```rust
#[cfg(test)]
mod tests
```

в отдельные файлы только для уменьшения line count.

Отдельные integration tests имеют смысл только тогда, когда тестируют public subsystem boundaries.

Unit tests конкретных controller/filter/data structures могут оставаться рядом с реализацией.

---

## 9. Проверить public API после структурного рефакторинга

После основных переносов выполнить отдельный audit:

* что действительно должно быть `pub`;
* что достаточно `pub(crate)`;
* что может быть private;
* нет ли helper functions, экспортированных только из-за прежнего расположения файлов;
* нет ли cyclic conceptual dependencies;
* не приходится ли sibling modules обращаться к внутренним деталям друг друга.

Не менять API массово без пользы.

Если есть достаточный набор очевидных изменений:

```text
refactor: tighten internal module boundaries
```

---

## 10. Финальная проверка

После всего structural pass:

```powershell
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
git diff --check
cargo build --release
```

Проверить вручную хотя бы:

```text
startup
profile load/reload
virtual instrument emulator
plotting
filters
Manual control
PID
OnOff
Furnace controller if exposed in current build
controller pause/resume
safe output
recording
shutdown
```

---

## Приоритет

Работать примерно в таком порядке:

```text
1. lua_api.rs
2. signal_processing/service.rs
3. output_control/service.rs
4. application_runtime.rs
5. process_control/controller.rs only if still justified
6. help/documentation
7. visibility/module-boundary cleanup
8. final verification
```

Но перед каждым пунктом сначала анализировать текущий код.

Если предполагаемый refactor не делает ownership, dependency direction или navigation явно лучше, пропустить его.

Основной принцип:

```text
large file is not automatically bad

mixed responsibilities are bad
```

Не стремиться к минимальному количеству строк в файле. Стремиться к тому, чтобы по имени модуля было понятно, чем он владеет и почему код находится именно там.
