#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

use com_port_reader::{app::MyApp, application_paths::ApplicationPaths};

fn main() -> eframe::Result<()> {
    let application_paths = match ApplicationPaths::discover() {
        Ok(paths) => paths,

        Err(error) => {
            let _ = rfd::MessageDialog::new()
                .set_title("COM Port Reader")
                .set_description(error.to_string())
                .set_level(rfd::MessageLevel::Error)
                .show();

            return Ok(());
        }
    };

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "COM Port Reader",
        options,
        Box::new(move |_cc| Ok(Box::new(MyApp::new(application_paths)))),
    )
}
