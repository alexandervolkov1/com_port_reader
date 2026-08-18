---@meta
---@diagnostic disable: missing-return

---@alias SerialParity
---| '"none"'
---| '"even"'
---| '"odd"'

---@alias SerialFlowControl
---| '"none"'
---| '"software"'
---| '"hardware"'

---@alias ParameterAccess
---| '"read_only"'
---| '"write_only"'
---| '"read_write"'

---@alias ParameterValueType
---| '"boolean"'
---| '"integer"'
---| '"number"'

---@alias InstrumentValue
---| boolean
---| integer
---| number

---@alias ControlPanelValue
---| string
---| number
---| boolean

---Line color written as a hexadecimal RGB value such as "#1A2B3C".
---@alias SeriesColor string

---@class RuntimeDefinition
---@field fps? integer GUI refresh rate. Default: 30.
---@field poll_interval? number Default series polling interval in seconds. Default: 1.0.
---@field plot_window? number Live plot window in seconds. Default: 3600.0.
---@field max_plot_points_per_series? integer Maximum number of points prepared for one visible series. Default: 4000.

---@class SerialConnectionDefinition
---@field port string COM port name.
---@field baud_rate? integer Baud rate. Default: 9600.
---@field data_bits? integer Data bits: 5, 6, 7 or 8. Default: 8.
---@field parity? SerialParity Default: "none".
---@field stop_bits? integer Stop bits: 1 or 2. Default: 1.
---@field flow_control? SerialFlowControl Default: "none".
---@field timeout? number Read timeout in seconds. Default: 0.25.

---@class EmulatorDefinition
---@field connection string Name of the client connection whose serial-line settings are used.
---@field port string Server side of the virtual COM-port pair.
---@field script string Path to the Lua virtual-instrument model.

---@class ApplicationDefinition
---@field application? RuntimeDefinition
---@field connections? table<string, SerialConnectionDefinition>
---@field emulator? EmulatorDefinition
---@field scripts? string[] Application scripts executed after setup().
---@field setup? fun() Called once after the application Lua API is installed.

---@class SeriesOptions
---@field name? string Optional unique series name.
---@field interval? number Polling interval in seconds. The application default is used when omitted.
---@field color? SeriesColor Line color in #RRGGBB format. An automatic color is used when omitted.

---@class SerialSeriesOptions: SeriesOptions
---@field connection? string Serial connection name. Default: "primary".

---@class SerialCommandOptions
---@field connection? string Serial connection name. Default: "primary".

---@class Metakon5x3Options
---@field connection? string Serial connection name. Default: "primary".
---@field device? integer Device address from 0 to 255. Default: 1.
---@field channel? integer Device channel from 0 to 255. Default: 0.
---@field scale? number Positive multiplier applied to scaled parameters. Default: 1.0.

---Typed Metakon 5X3 parameter key.
---
---Driver-specific behavior:
---* integral_time is exposed in minutes; the raw device register uses seconds;
---* measurement value -32768 is treated as a sensor fault, not as a temperature;
---* the device's front-panel integral OFF state is not exposed by integral_time,
---  so reading it returns the last stored numeric value.
---@alias Metakon5x3Parameter
---| '"channel_type"'
---| '"measurement"'
---| '"setpoint"'
---| '"proportional_band"'
---| '"integral_time"'
---| '"derivative_time"'
---| '"output_power"'
---| '"pwm_positive"'
---| '"pwm_negative"'
---| '"upper_setpoint"'
---| '"upper_hysteresis"'
---| '"upper_output"'
---| '"lower_setpoint"'
---| '"lower_hysteresis"'
---| '"lower_output"'

---@class InstrumentParameterInfo
---@field key string Machine-readable parameter key.
---@field name string Human-readable parameter name.
---@field access ParameterAccess
---@field value_type ParameterValueType
---@field series? boolean Whether the parameter can be added as a periodic series.
---@field unit? string Physical unit.
---@field minimum? number Minimum accepted value.
---@field maximum? number Maximum accepted value.
---@field scale? number Scale used by the typed instrument driver.

---@class Metakon5x3
local Metakon5x3 = {}

---Returns the typed Metakon parameter descriptors.
---@return InstrumentParameterInfo[]
function Metakon5x3:parameters() end

---Adds a periodically sampled Metakon parameter.
---
---The second argument may be:
---* omitted;
---* a series name string;
---* a SeriesOptions table.
---@param parameter Metakon5x3Parameter
---@param options? string|SeriesOptions
function Metakon5x3:add(
    parameter,
    options
)
end

---Reads one Metakon parameter immediately.
---
---The operation is queued behind periodic polling
---that is already due. The result or communication
---error is also written to the application log.
---A successful read also re-enables periodic polling
---for a suspended series of the same parameter.
---@param parameter Metakon5x3Parameter
---@return InstrumentValue
function Metakon5x3:read(parameter) end

---Writes one writable Metakon parameter.
---
---After writing, the driver reads the parameter back
---and returns the actual value reported by the device.
---@param parameter Metakon5x3Parameter
---@param value InstrumentValue
---@return InstrumentValue
function Metakon5x3:write(
    parameter,
    value
)
end

---@class VirtualInstrumentOptions
---@field connection? string Serial connection name. Default: "primary".
---@field id? integer One-based virtual instrument ID. Default: 1.

---@class VirtualInstrument
local VirtualInstrument = {}

---Returns the one-based virtual instrument ID.
---@return integer
function VirtualInstrument:id() end

---Returns the instrument name declared by its Lua model.
---@return string
function VirtualInstrument:name() end

---Returns parameter descriptors discovered from the emulator.
---@return InstrumentParameterInfo[]
function VirtualInstrument:parameters() end

---Adds a readable series-enabled virtual parameter.
---
---The second argument may be:
---* omitted;
---* a series name string;
---* a SeriesOptions table.
---@param parameter string
---@param options? string|SeriesOptions
function VirtualInstrument:add(
    parameter,
    options
)
end

---Reads one virtual instrument parameter immediately.
---@param parameter string
---@return InstrumentValue
function VirtualInstrument:read(parameter) end

---Writes one writable virtual instrument parameter.
---
---Returns the actual value returned by the virtual model.
---@param parameter string
---@param value InstrumentValue
---@return InstrumentValue
function VirtualInstrument:write(
    parameter,
    value
)
end

---@class ControlReadoutDefinition
---@field kind '"readout"'
---@field id string
---@field label string
---@field initial? string

---@class ControlNumberDefinition
---@field kind '"number"'
---@field id string
---@field label string
---@field initial? number
---@field min? number
---@field max? number
---@field step? number
---@field on_change string Callback field in the application script table.

---@class ControlToggleDefinition
---@field kind '"toggle"'
---@field id string
---@field label string
---@field initial? boolean
---@field on_change string Callback field in the application script table.

---@class ControlButtonDefinition
---@field kind '"button"'
---@field id string
---@field label string
---@field on_click string Callback field in the application script table.

---@alias ControlDefinition
---| ControlReadoutDefinition
---| ControlNumberDefinition
---| ControlToggleDefinition
---| ControlButtonDefinition

---@class ControlPanelDefinition
---@field id string
---@field title string
---@field controls ControlDefinition[]

---@class ApplicationScript
---@field id string
---@field panels? ControlPanelDefinition[]
---@field [string] any Named Lua callbacks may be stored in the script table.

---@class ApplicationApi
app = {}

---Starts periodic acquisition on every configured connection.
function app.start() end

---Stops periodic acquisition on every configured connection.
function app.stop() end

---Removes every series and all accumulated samples.
function app.clear() end

---Writes an informational message to the application log.
---@param message string
function app.log(message) end

---Starts the emulator configured by the active startup profile.
function app.start_emu() end

---Stops the running device emulator.
function app.stop_emu() end

---Adds a periodically sampled text-command serial series.
---
---The response must contain one finite number.
---
---The second argument may be:
---* omitted;
---* a series name string;
---* a SerialSeriesOptions table.
---@param command string
---@param options? string|SerialSeriesOptions
function app.add_serial(
    command,
    options
)
end

---Creates a typed Metakon 5X3 controller.
---@param options? Metakon5x3Options
---@return Metakon5x3
function app.metakon(options) end

---Discovers and creates a virtual instrument controller.
---
---The server or device emulator must already be running.
---@param options? VirtualInstrumentOptions
---@return VirtualInstrument
function app.virtual_instrument(options) end

---Deletes a series by name.
---@param name string
function app.delete(name) end

---Renames an existing series.
---@param current_name string
---@param new_name string
function app.rename(
    current_name,
    new_name
)
end

---Changes the line color of an existing series.
---
---Pass nil to restore automatic color selection.
---@param name string Existing unique series name.
---@param color? SeriesColor Line color in #RRGGBB format, or nil for automatic color.
function app.set_color(
    name,
    color
)
end

---Re-enables periodic polling for a suspended series.
---
---The series is identified by its unique name.
---The next request follows the normal polling schedule.
---If communication fails three more consecutive times,
---the series is suspended again.
---@param name string
function app.retry(name) end

---Re-enables periodic polling for every suspended series.
---
---Existing samples are preserved. Acquisition is not
---stopped or restarted.
function app.retry_all() end

---Sends one text command through a serial connection.
---
---The response or communication error is written
---to the application log.
---@param command string
---@param options? SerialCommandOptions
function app.send_serial(
    command,
    options
)
end

---Registers an application script and publishes
---its declarative control panels to the GUI.
---
---Callback names declared by on_change and on_click
---must refer to functions stored in this script table.
---@param script ApplicationScript
function app.register_script(script) end

---Removes a registered application script
---and all control panels belonging to it.
---@param script_id string
function app.unregister_script(script_id) end

---Updates a control displayed in the GUI.
---
---Accepted value types:
---* readout: string;
---* number: number;
---* toggle: boolean.
---
---Button controls cannot receive values.
---@param script_id string
---@param panel_id string
---@param control_id string
---@param value ControlPanelValue
function app.set_control(
    script_id,
    panel_id,
    control_id,
    value
)
end
