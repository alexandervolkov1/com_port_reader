use eframe::egui;

use crate::{application_runtime::ApplicationRuntime, user_command::UserCommand};

pub fn show(ui: &mut egui::Ui, runtime: &mut ApplicationRuntime) {
    ui.horizontal(|ui| {
        let running = runtime.is_running();

        if ui
            .add_enabled(!running, egui::Button::new("Start"))
            .clicked()
        {
            runtime.execute(UserCommand::Start);
        }

        if ui.add_enabled(running, egui::Button::new("Stop")).clicked() {
            runtime.execute(UserCommand::Stop);
        }

        if running {
            ui.colored_label(egui::Color32::from_rgb(0, 150, 0), "Signals: ● Running");
        } else {
            ui.colored_label(egui::Color32::GRAY, "Signals: ■ Stopped");
        }
    });
}
