#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod data;
mod ui;
mod utils;
mod worker;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "COM Port Reader",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(app::MyApp::new()))),
    )
}
