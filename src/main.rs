#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod acquisition;
mod app;
mod app_config;
mod app_log;
mod components;
mod data;
mod device_emulator;
mod device_emulator_handle;
pub mod device_model;
mod lua_api;
pub mod lua_device_model;
mod lua_execution;
pub mod lua_runtime;
pub mod lua_worker;
pub mod protocol;
mod sample_sink;
mod serial_connection;
mod user_command;
mod utils;
mod worker;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "COM Port Reader",
        options,
        Box::new(|_cc| Ok(Box::new(app::MyApp::new()))),
    )
}
