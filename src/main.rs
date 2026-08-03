#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "COM Port Reader",
        options,
        Box::new(|_cc| Ok(Box::new(com_port_reader::app::MyApp::new()))),
    )
}
