local PLANT_ID = 1

local TEMPERATURE_SERIES_NAME =
    "thermal_temperature"

local POWER_SERIES_NAME =
    "thermal_heater_power"

local PID_NAME =
    "thermal_temperature_pid"

local POLL_INTERVAL = 0.5

local SETPOINT = 150.0

-- PI controller parameters.
--
-- The virtual plant has:
--   time constant = 30 s
--   heater gain   = 2 °C / %
--
-- These deliberately conservative values should
-- produce a smooth approach to the setpoint.
local KP = 1.0
local KI = 1.0 / 30.0
local KD = 0.0

local OUTPUT_MIN = 0.0
local OUTPUT_MAX = 100.0


app.stop()
app.stop_emu()
app.clear()

app.start_emu()


local plant =
    app.virtual_instrument({
        id = PLANT_ID,
    })


plant:add(
    "temperature",
    {
        name = TEMPERATURE_SERIES_NAME,
        interval = POLL_INTERVAL,
        color = "#D32F2F",
    }
)


plant:add(
    "heater_power",
    {
        name = POWER_SERIES_NAME,
        interval = POLL_INTERVAL,
        color = "#1976D2",
    }
)


plant:pid(
    "heater_power",
    {
        name = PID_NAME,

        input =
            TEMPERATURE_SERIES_NAME,

        setpoint = SETPOINT,

        kp = KP,
        ki = KI,
        kd = KD,

        output_min = OUTPUT_MIN,
        output_max = OUTPUT_MAX,
    }
)


app.log(
    "Thermal PID demo configured: "
        .. "setpoint = "
        .. SETPOINT
        .. " °C."
)


app.start()
