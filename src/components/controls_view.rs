use eframe::egui;

use crate::{application_runtime::ApplicationRuntime, user_command::AcquisitionCommand};

pub fn show(ui: &mut egui::Ui, runtime: &mut ApplicationRuntime) {
    ui.horizontal(|ui| {
        let running = runtime.is_running();

        if ui
            .add_enabled(!running, egui::Button::new("Start"))
            .clicked()
        {
            runtime.execute(AcquisitionCommand::Start.into());
        }

        if ui.add_enabled(running, egui::Button::new("Stop")).clicked() {
            runtime.execute(AcquisitionCommand::Stop.into());
        }

        if running {
            ui.colored_label(egui::Color32::from_rgb(0, 150, 0), "Signals: ● Running");
        } else {
            ui.colored_label(egui::Color32::GRAY, "Signals: ■ Stopped");
        }
    });
}
