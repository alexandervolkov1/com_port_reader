---@meta

---@class Metakon5x3Options
---@field device? integer Device address from 0 to 255. Default: 1.
---@field channel? integer Device channel from 0 to 255. Default: 0.
---@field scale? number Multiplier applied to measurement and setpoint series. Default: 1.0.

---@class Metakon5x3
local Metakon5x3 = {}

---Adds the measured-value series to periodic acquisition.
---@param name? string Optional unique series name.
function Metakon5x3:add_measurement(name) end

---Adds the PID setpoint series to periodic acquisition.
---@param name? string Optional unique series name.
function Metakon5x3:add_setpoint(name) end

---Adds the output-power series to periodic acquisition.
---
---Output power is reported in the range from -100 to 100.
---@param name? string Optional unique series name.
function Metakon5x3:add_output_power(name) end

---Adds the PID proportional-band series.
---@param name? string
function Metakon5x3:add_proportional_band(name) end

---Adds the PID integral-time series.
---@param name? string
function Metakon5x3:add_integral_time(name) end

---Adds the PID derivative-time series.
---@param name? string
function Metakon5x3:add_derivative_time(name) end

---Changes the PID setpoint.
---
---The value must be an integer from -999 to 9999.
---The controller scale is not applied when writing.
---@param value integer
function Metakon5x3:setpoint(value) end

---Changes the PID proportional band.
---
---The value must be an integer from 1 to 9999.
---@param value integer
function Metakon5x3:proportional_band(value) end

---Changes the PID integral time.
---
---The value is specified in seconds and must be
---an integer from 1 to 30000.
---@param value integer
function Metakon5x3:integral_time(value) end

---Changes the PID derivative time.
---
---The value is specified in seconds and must be
---an integer from 0 to 255.
---@param value integer
function Metakon5x3:derivative_time(value) end

---@class ApplicationApi
app = {}

---Starts periodic data acquisition.
function app.start() end

---Stops periodic data acquisition.
---
---Active CSV recording remains open but is paused.
function app.stop() end

---Removes all series and their accumulated samples.
function app.clear() end

---Starts CSV recording and creates a new protocol file.
function app.start_rec() end

---Stops CSV recording and closes the current protocol file.
function app.stop_rec() end

---Starts the selected device emulator.
function app.start_emu() end

---Stops the running device emulator.
function app.stop_emu() end

---Adds a periodically sampled text-command serial series.
---
---The response must contain one finite number.
---@param command string Command sent during every acquisition cycle.
---@param name? string Optional unique series name.
function app.add_serial(
    command,
    name
)
end

---Creates a typed Metakon 5X3 controller.
---
---Default values:
---device = 1
---channel = 0
---scale = 1.0
---@param options? Metakon5x3Options
---@return Metakon5x3
function app.metakon(options) end

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

---Sends one text command through the selected serial port.
---
---The response or communication error is written to
---the application log.
---@param command string Command text.
function app.send_serial(command) end
