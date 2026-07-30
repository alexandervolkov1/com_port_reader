---@meta

---@alias MetakonValueType
---| "ubyte"
---| "byte"
---| "uint"
---| "int"

---@class MetakonSeriesOptions
---@field device? integer Device address, from 0 to 255.
---@field channel? integer Device channel, from 0 to 255.
---@field register? integer Register address, from 0 to 255.
---@field value_type? MetakonValueType Register data type. Default: "int".
---@field scale? number Multiplier applied to the raw register value.
---@field name? string Optional unique series name.

---@class MetakonSetpointOptions
---@field device? integer Device address. Default: 1.
---@field channel? integer Device channel. Default: 0.
---@field value integer Raw Int register value.

---@class ApplicationApi
app = {}

---Starts data acquisition.
function app.start() end

---Stops data acquisition.
function app.stop() end

---Removes all series and their samples.
function app.clear() end

---Starts CSV recording.
function app.start_rec() end

---Stops CSV recording and closes the protocol file.
function app.stop_rec() end

---Starts the selected device emulator.
function app.start_emu() end

---Stops the running device emulator.
function app.stop_emu() end

---Adds a periodically sampled serial series.
---@param command string Command sent during every acquisition cycle.
---@param name? string Optional unique series name.
function app.add_serial(
    command,
    name
)
end

---Adds a periodically sampled Metakon register.
---
---Default values:
---device = 1
---channel = 0
---register = 0x01
---scale = 1.0
---@param options? MetakonSeriesOptions
function app.add_metakon(options) end

---Writes the PID setpoint register 0x02.
---@param options MetakonSetpointOptions
function app.set_metakon_setpoint(options) end

---Deletes a series by name.
---@param name string Existing series name.
function app.delete(name) end

---Renames an existing series.
---@param current_name string Existing series name.
---@param new_name string New unique series name.
function app.rename(
    current_name,
    new_name
)
end

---Sends one serial command and writes the response to the application log.
---@param command string Command text.
function app.send_serial(command) end
