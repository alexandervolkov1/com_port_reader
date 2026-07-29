---@meta

---@class ApplicationApi
app = {}

---Starts data acquisition.
function app.start() end

---Stops data acquisition.
function app.stop() end

---Removes all series and samples.
function app.clear() end

---Starts CSV recording.
function app.start_rec() end

---Stops CSV recording.
function app.stop_rec() end

---Starts the selected device emulator.
function app.start_emu() end

---Stops the device emulator.
function app.stop_emu() end

---Adds a periodically sampled serial series.
---@param command string
---@param name? string
function app.add_serial(command, name) end

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

---Sends one serial command and writes its response to the log.
---@param command string
function app.send_serial(command) end
