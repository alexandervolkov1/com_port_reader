use eframe::egui;

use crate::components::controls_model::ControlsModel;

pub fn show(ui: &mut egui::Ui, controls: &mut ControlsModel) {
    ui.horizontal(|ui| {
        let running = controls.is_running();

        if ui
            .add_enabled(!running, egui::Button::new("Start"))
            .clicked()
        {
            controls.start();
        }

        if ui.add_enabled(running, egui::Button::new("Stop")).clicked() {
            controls.stop();
        }

        if ui.button("Clear").clicked() {
            controls.clear();
        }

        if running {
            ui.colored_label(egui::Color32::from_rgb(0, 150, 0), "Signals: ● Running");
        } else {
            ui.colored_label(egui::Color32::GRAY, "Signals: ■ Stopped");
        }
    });

    ui.horizontal(|ui| {
        let recording = controls.is_recording();

        if ui
            .add_enabled(!recording, egui::Button::new("Start recording"))
            .clicked()
        {
            controls.start_recording();
        }

        if ui
            .add_enabled(recording, egui::Button::new("Stop recording"))
            .clicked()
        {
            controls.stop_recording();
        }

        match (recording, controls.is_running()) {
            (true, true) => {
                ui.colored_label(egui::Color32::from_rgb(190, 30, 30), "CSV: ● Writing");
            }

            (true, false) => {
                ui.colored_label(egui::Color32::from_rgb(190, 130, 0), "CSV: ‖ Paused");
            }

            (false, _) => {
                ui.colored_label(egui::Color32::GRAY, "CSV: ■ Off");
            }
        }
    });

    if let Some(path) = controls.recording_file() {
        ui.label(format!("Protocol: {}", path.display()));
    }

    if let Some(error) = controls.recording_error() {
        ui.colored_label(egui::Color32::RED, error);
    }
}
