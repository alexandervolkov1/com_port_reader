#![cfg_attr(windows, windows_subsystem = "windows")]

mod acquisition;
mod app;
mod components;
mod data;
mod dsl;
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
