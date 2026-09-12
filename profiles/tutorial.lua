-- An empty experiment for docs/lua-tutorial.md; start the model from the console.
return {
    application = { poll_interval = 0.5 },
    emulator = {
        transport = "memory",
        script = "../emulator_scripts/furnace_plant.lua",
    },
}
