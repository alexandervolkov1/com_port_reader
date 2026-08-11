use eframe::egui;

use crate::{
    application_runtime::{ApplicationRuntime, RecordingTransition},
    user_command::UserCommand,
};

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

        if ui.button("Clear").clicked() {
            runtime.execute(UserCommand::Clear);
        }

        if running {
            ui.colored_label(egui::Color32::from_rgb(0, 150, 0), "Signals: ● Running");
        } else {
            ui.colored_label(egui::Color32::GRAY, "Signals: ■ Stopped");
        }
    });

    ui.horizontal(|ui| {
        let recording = runtime.is_recording();

        let transition = runtime.recording_transition();

        let transition_pending = transition.is_some();

        if ui
            .add_enabled(
                !recording && !transition_pending,
                egui::Button::new("Start recording"),
            )
            .clicked()
        {
            runtime.execute(UserCommand::StartRecording);
        }

        if ui
            .add_enabled(
                recording && !transition_pending,
                egui::Button::new("Stop recording"),
            )
            .clicked()
        {
            runtime.execute(UserCommand::StopRecording);
        }

        match transition {
            Some(RecordingTransition::Starting) => {
                ui.colored_label(egui::Color32::from_rgb(190, 130, 0), "CSV: … Starting");
            }

            Some(RecordingTransition::Stopping) => {
                ui.colored_label(egui::Color32::from_rgb(190, 130, 0), "CSV: … Stopping");
            }

            None => match (recording, runtime.is_running()) {
                (true, true) => {
                    ui.colored_label(egui::Color32::from_rgb(190, 30, 30), "CSV: ● Writing");
                }

                (true, false) => {
                    ui.colored_label(egui::Color32::from_rgb(190, 130, 0), "CSV: ‖ Paused");
                }

                (false, _) => {
                    ui.colored_label(egui::Color32::GRAY, "CSV: ■ Off");
                }
            },
        }
    });

    if let Some(path) = runtime.recording_file() {
        ui.label(format!("Protocol: {}", path.display(),));
    }

    if let Some(error) = runtime.recording_error() {
        ui.colored_label(egui::Color32::RED, error);
    }
}
