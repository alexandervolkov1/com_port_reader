Нужно мягко убрать обязательную зависимость эмулятора от внешней пары виртуальных COM-портов.

## Цель

Сейчас схема примерно такая:

```text
application
    ↓
SerialConnection
    ↓
virtual COM A
    ⇅
virtual COM B
    ↓
DeviceEmulator
    ↓
VirtualInstrumentServer
    ↓
Lua model
```

Нужно получить основной режим:

```text
application
    ↓
AcquisitionSource
    ↓
in-memory transport
    ↓
VirtualInstrumentServer
    ↓
Lua model
```

без установки com0com/VSPE/другого virtual COM driver.

При этом старый serial-emulator mode желательно сохранить как optional integration-test mode.

## Главные ограничения

Это не должен быть общий rewrite acquisition architecture.

Не менять без необходимости:

* real COM/RS-485 behavior;
* Metakon driver;
* controller/output-control semantics;
* Lua API;
* series semantics;
* process recorder;
* worker scheduling;
* polling failure/retry logic;
* VirtualInstrument Lua model format.

Особенно не трогать рабочий путь:

```text
Metakon
    ↓
SerialCommandSource
    ↓
SerialConnection
    ↓
real COM
```

Правило работы:

```text
1 логический шаг = 1 commit
```

После каждого шага:

```powershell
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
git diff --check
```

Перед началом:

```powershell
git status
git log --oneline -15
```

---

# Шаг 1. Изучить существующую границу AcquisitionSource

Сначала не писать код.

Проверить:

* `AcquisitionSource`;
* `CombinedSource`;
* `SerialCommandSource`;
* worker construction;
* `VirtualInstrumentClient`;
* `VirtualInstrumentServer`;
* `DeviceEmulatorHandle`;
* `DeviceEmulatorService`;
* emulator configuration parsing.

Сейчас `CombinedSource` уже умеет последовательно маршрутизировать:

```text
sample_series
describe_virtual_instruments
read_instrument
write_instrument
request_text
```

между несколькими `AcquisitionSource`.

Использовать эту существующую архитектуру, а не создавать параллельную систему.

Определить минимальную точку, куда можно добавить:

```text
LocalVirtualInstrumentSource
```

или эквивалент.

На этом шаге код не менять.

---

# Шаг 2. Добавить локальный источник виртуальных приборов

Создать отдельный `AcquisitionSource`, отвечающий только за virtual instruments.

Примерная ответственность:

```text
LocalVirtualInstrumentSource

supports:
    VirtualInstrument series
    describe_virtual_instruments
    VirtualInstrument read
    VirtualInstrument write

does not support:
    raw serial text commands
    Metakon requests
    unrelated series
```

Для неподдерживаемых операций возвращать `Ok(None)`, как принято в `AcquisitionSource`.

Не переносить virtual-instrument-specific knowledge обратно в generic worker.

Commit:

```text
feat: add local virtual instrument source
```

На этом этапе источник можно тестировать отдельно, ещё не включая его в normal runtime.

---

# Шаг 3. Добавить in-memory duplex transport

Нужен внутренний transport между virtual-instrument client и emulator server.

Не пытаться создавать настоящий Windows COM-port.

Сделать примерно:

```text
MemoryEndpoint A
    ⇅
MemoryEndpoint B
```

где:

```text
A.write → B.read
B.write → A.read
```

Предпочтительно передавать блоки байтов, а не отдельные `u8`.

Transport должен корректно поддерживать:

* ordered delivery;
* partial reads;
* buffering;
* timeout;
* clean close/disconnect;
* repeated request/response traffic.

Не требуется эмулировать бессмысленные serial settings вроде parity/baud внутри memory mode.

Если существующий `VirtualInstrumentClient` слишком сильно зависит от `SerialConnection`, вынести **минимальный transport interface**, нужный только virtual-instrument protocol.

Например концептуально:

```rust
trait ByteTransport {
    fn read(...);
    fn write_all(...);
}
```

Не пытаться абстрагировать весь serial subsystem.

SerialConnection должен остаться одним implementation/adapter для real serial mode.

Commit:

```text
feat: add in-memory virtual instrument transport
```

Обязательные unit tests:

```text
client → server bytes
server → client bytes
large message split across reads
multiple sequential messages
timeout
disconnect
```

---

# Шаг 4. Перевести DeviceEmulator на transport abstraction

Сейчас emulator loop концептуально делает:

```text
read bytes
    ↓
frame decoder
    ↓
VirtualInstrumentMessage
    ↓
VirtualInstrumentServer
    ↓
response
    ↓
encode frame
    ↓
write bytes
```

Эту логику сохранить.

Она не должна знать, является transport:

```text
COM port
```

или:

```text
memory endpoint
```

Нужно получить примерно:

```text
run_emulator(transport, model)
```

вместо жёсткой зависимости от:

```text
Box<dyn SerialPort>
```

Старый serial start path пока сохранить.

То есть:

```text
DeviceEmulator
    ├── serial transport
    └── memory transport
```

Commit:

```text
refactor: decouple device emulator from serial port
```

Поведение serial emulator после этого commit должно остаться прежним.

---

# Шаг 5. Подключить LocalVirtualInstrumentSource через CombinedSource

Сейчас worker получает примерно:

```text
CombinedSource
    └── SerialCommandSource
```

Нужно для local emulator mode получить:

```text
CombinedSource
    ├── LocalVirtualInstrumentSource
    └── SerialCommandSource
```

Порядок важен.

Local source должен первым обрабатывать virtual-instrument requests, чтобы они не уходили в `SerialCommandSource`.

Metakon и обычные serial commands должны продолжать попадать в `SerialCommandSource`.

То есть одновременно должно работать:

```text
local virtual furnace
        +
real Metakon on COM3
```

в одном приложении.

Не переносить эту маршрутизацию в Lua API.

Commit:

```text
feat: route local emulator through acquisition source
```

---

# Шаг 6. Продумать lifecycle start/stop/restart

Это критичный кусок.

Текущий API:

```lua
app.start_emu()
app.stop_emu()
```

оставить без изменений.

Memory mode должен корректно поддерживать:

```text
start
stop
start again
profile reload
application shutdown
```

После `app.stop_emu()` virtual-instrument access должен выдавать понятную ошибку, а не зависать.

После повторного `app.start_emu()` новый Lua model должен начинать с нового состояния.

Не оставлять background thread или channel, который пережил старый emulator instance случайно.

Добавить tests для:

```text
start → read → stop
start → stop → start → read
drop runtime while emulator runs
profile reload while emulator runs
```

Этот шаг можно включить в предыдущий commit, если lifecycle естественно является частью integration. Не создавать искусственный commit ради количества.

---

# Шаг 7. Убрать обязательный emulator COM port из configuration

Сейчас emulator configuration требует server-side virtual COM port.

Нужно сделать memory mode основным.

Желаемый пользовательский вариант:

```lua
emulator = {
    script = "emulator_scripts/furnace_plant.lua",
}
```

или, если нужен явный transport:

```lua
emulator = {
    transport = "memory",
    script = "emulator_scripts/furnace_plant.lua",
}
```

Memory желательно сделать default.

Старый integration mode можно сохранить:

```lua
emulator = {
    transport = "serial",
    connection = "primary",
    port = "COM4",
    script = "emulator_scripts/furnace_plant.lua",
}
```

Не сохранять обязательное поле `port` в memory mode.

Также проверить вопрос logical connection.

Emulator-only profile не должен требовать фиктивный системный COM-port только ради создания worker.

Перед изменением архитектуры connection definitions посмотреть, нельзя ли использовать уже существующую возможность `SerialConfigStore` жить без открытого порта и создать logical worker минимальным изменением.

Не вводить большой новый `Connection` framework, если задача решается меньшим изменением.

Commit:

```text
feat: make in-memory emulator transport the default
```

---

# Шаг 8. Emulator-only profile без COM

Добавить end-to-end test/profile, который запускает virtual furnace без единого virtual COM port.

Например смысл профиля:

```lua
return {
    emulator = {
        script = "emulator_scripts/furnace_plant.lua",
    },

    setup = function()
        app.start_emu()

        local furnace =
            app.virtual_instrument({ id = 1 })

        furnace:add("temperature", {
            name = "temperature",
            interval = 0.1,
        })

        app.start()
    end,
}
```

Тест должен доказать:

```text
profile loads
emulator starts
instrument discovery works
read works
write works
periodic acquisition works
controller output can write heater_power
shutdown is clean
```

В тесте не должно быть:

```text
COM255
COM3
COM4
com0com
```

Commit при необходимости:

```text
test: cover emulator without serial ports
```

Если тест естественно входит в feature commit, отдельный commit не нужен.

---

# Шаг 9. Сохранить serial emulator как integration path

Не удалять текущий serial emulator сразу.

Он полезен для проверки:

```text
framing
serial I/O
VirtualInstrumentClient
VirtualInstrumentServer
real Windows virtual COM behavior
```

Но он больше не должен быть нужен обычному пользователю.

Можно оставить:

```text
src/bin/device_emulator.rs
```

как standalone integration/debug tool.

Итог:

```text
normal development:
    memory emulator

serial protocol integration test:
    virtual COM pair

real laboratory:
    physical COM / RS-485
```

---

# Шаг 10. Обновить profiles/help

Перевести demo profiles:

```text
furnace
PID thermal
on/off thermal
virtual sine
```

на memory emulator mode.

Удалить из обычной документации требование заранее создавать virtual COM pair.

Serial emulator описать отдельно как advanced/testing mode.

Не менять Lua device-model format:

```text
instruments
read()
write()
```

остаётся тем же.

Commit:

```text
docs: update emulator transport configuration
```

---

# Финальная архитектура

Цель:

```text
                         Acquisition worker
                                │
                                ▼
                         CombinedSource
                         /            \
                        /              \
       LocalVirtualInstrumentSource   SerialCommandSource
                    │                       │
                    ▼                       ▼
             memory transport          SerialConnection
                    │                       │
                    ▼                       ▼
        VirtualInstrumentServer          real COM
                    │                       │
                    ▼                       ▼
               Lua model                 Metakon
```

При этом верхние subsystems не должны знать, откуда пришёл прибор:

```text
Lua application API
Series
Filters
Controllers
OutputControl
Recorder
Plots
```

для них local virtual instrument должен выглядеть так же, как сейчас virtual instrument через serial transport.

---

# Что не делать

Не:

```text
писать Windows virtual COM driver
создавать kernel device
тащить serialport::SerialPort abstraction во все subsystems
делать generic transport framework для будущих TCP/USB/Bluetooth "на всякий случай"
переписывать worker
переписывать Lua API
удалять serial emulator до появления memory-mode tests
```

Решаем одну конкретную задачу:

```text
virtual instrument emulator
не должен требовать внешний virtual COM driver
```

и используем уже существующие architectural seams.

---

# Финальная проверка

После завершения:

```powershell
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
git diff --check
cargo build --release
```

Ручная проверка:

```text
furnace demo without virtual COM
PID demo
on/off demo
emulator stop/start
profile reload
manual read/write
periodic acquisition
controller writes
process recording
clean shutdown
real COM/Metakon smoke test
```

Основной критерий готовности:

```text
чистая Windows-машина без virtual COM software
должна запускать все emulator demos
сразу после установки приложения.
```
