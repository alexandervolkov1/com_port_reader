---@meta

---@alias MetakonValueType
---| "ubyte" Unsigned 8-bit integer.
---| "byte" Signed 8-bit integer.
---| "uint" Unsigned 16-bit integer.
---| "int" Signed 16-bit integer.

---@class MetakonSeriesOptions
---@field device? integer Device address, from 0 to 255. Default: 1.
---@field channel? integer Device channel, from 0 to 255. Default: 0.
---@field register? integer Register address, from 0 to 255. Default: 0x01.
---@field value_type? MetakonValueType Register value type. Default: "int".
---@field scale? number Multiplier applied to the raw register value. Default: 1.0.
---@field name? string Optional unique series name.

---@class MetakonSetpointOptions
---@field device? integer Device address, from 0 to 255. Default: 1.
---@field channel? integer Device channel, from 0 to 255. Default: 0.
---@field value integer Raw Int register value, from -999 to 9999.

---@class MetakonParameterOptions
---@field device? integer Device address, from 0 to 255. Default: 1.
---@field channel? integer Device channel, from 0 to 255. Default: 0.
---@field value integer Raw register value.

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
---value_type = "int"
---scale = 1.0
---@param options? MetakonSeriesOptions
function app.add_metakon(options) end

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

---Writes the PID setpoint to Metakon register 0x02.
---
---The value is written as a raw signed 16-bit integer.
---Allowed range: -999 to 9999.
---@param options MetakonSetpointOptions
function app.set_metakon_setpoint(options) end

---Writes the proportional band to Metakon register 0x03.
---
---The value is written as an unsigned 16-bit integer.
---Allowed range: 1 to 9999.
---@param options MetakonParameterOptions
function app.set_metakon_proportional_band(options) end

---Writes the integration time to Metakon register 0x04.
---
---The value is expressed in seconds and written as an unsigned
---16-bit integer.
---Allowed range: 1 to 30000.
---@param options MetakonParameterOptions
function app.set_metakon_integral_time(options) end

---Writes the derivative time to Metakon register 0x05.
---
---The value is written as an unsigned 8-bit integer.
---Allowed range: 0 to 255.
---@param options MetakonParameterOptions
function app.set_metakon_derivative_time(options) end
