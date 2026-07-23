#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod acquisition;
mod app;
mod components;
mod data;
mod dsl;
mod sample_sink;
mod script;
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
