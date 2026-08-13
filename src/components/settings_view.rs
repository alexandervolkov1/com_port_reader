use std::time::Duration;

use eframe::egui;

use crate::{
    application_definition::ApplicationDefinition,
    application_paths::ApplicationPaths,
    application_runtime::ApplicationRuntime,
    components::settings_model::{SettingsModel, SettingsReloadStatus, SettingsValidation},
};

pub fn show_menu_button(ui: &mut egui::Ui, model: &mut SettingsModel) {
    if ui.selectable_label(model.is_open(), "Settings").clicked() {
        model.toggle();
    }
}

pub fn show_window(
    context: &egui::Context,
    model: &mut SettingsModel,
    runtime: &ApplicationRuntime,
) {
    if !model.is_open() {
        return;
    }

    let mut open = true;

    egui::Window::new("Settings")
        .open(&mut open)
        .default_width(520.0)
        .resizable(true)
        .show(context, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    show_validation(ui, model, runtime);

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    show_configuration_paths(ui, runtime.paths());

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    show_runtime(ui, runtime.definition());

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    show_connections(ui, runtime.definition());

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    show_emulator(ui, runtime.definition(), runtime.paths());
                });
        });

    model.set_open(open);
}

fn show_configuration_paths(ui: &mut egui::Ui, paths: &ApplicationPaths) {
    ui.heading("Configuration");

    egui::Grid::new("settings_configuration_paths")
        .num_columns(2)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            ui.label("Startup file");
            ui.monospace(paths.startup_script().display().to_string());
            ui.end_row();

            ui.label("Root directory");
            ui.monospace(paths.profile_directory().display().to_string());
            ui.end_row();

            ui.label("Application directory:");
            ui.monospace(paths.application_directory().display().to_string());
            ui.end_row();
        });
}

fn show_runtime(ui: &mut egui::Ui, definition: &ApplicationDefinition) {
    let runtime = definition.runtime();

    ui.heading("Application");

    egui::Grid::new("settings_application")
        .num_columns(2)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            ui.label("FPS");
            ui.label(runtime.fps().to_string());
            ui.end_row();

            ui.label("Default polling interval");
            ui.label(format_duration(runtime.default_poll_interval()));
            ui.end_row();

            ui.label("Plot window");
            ui.label(format_duration(runtime.plot_window()));
            ui.end_row();

            ui.label("Maximum plot points per series");
            ui.label(runtime.max_plot_points_per_series().to_string());
            ui.end_row();
        });
}

fn show_connections(ui: &mut egui::Ui, definition: &ApplicationDefinition) {
    ui.heading("Serial connections");

    if definition.serial_connections().is_empty() {
        ui.weak("No serial connections configured.");

        return;
    }

    for connection in definition.serial_connections() {
        let config = connection.serial_config();

        egui::CollapsingHeader::new(format!("{} — {}", connection.name(), config.port_name(),))
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new(("settings_serial_connection", connection.id().value()))
                    .num_columns(2)
                    .spacing([16.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Connection ID");
                        ui.label(connection.id().to_string());
                        ui.end_row();

                        ui.label("Name");
                        ui.label(connection.name());
                        ui.end_row();

                        ui.label("Port");
                        ui.monospace(config.port_name());
                        ui.end_row();

                        ui.label("Baud rate");
                        ui.label(config.baud_rate().to_string());
                        ui.end_row();

                        ui.label("Data bits");
                        ui.label(format!("{:?}", config.data_bits()));
                        ui.end_row();

                        ui.label("Parity");
                        ui.label(format!("{:?}", config.parity()));
                        ui.end_row();

                        ui.label("Stop bits");
                        ui.label(format!("{:?}", config.stop_bits()));
                        ui.end_row();

                        ui.label("Flow control");
                        ui.label(format!("{:?}", config.flow_control()));
                        ui.end_row();

                        ui.label("Timeout");
                        ui.label(format!("{} ms", config.timeout_ms()));
                        ui.end_row();
                    });
            });
    }
}

fn show_emulator(ui: &mut egui::Ui, definition: &ApplicationDefinition, paths: &ApplicationPaths) {
    ui.heading("Device emulator");

    let Some(emulator) = definition.emulator() else {
        ui.weak("Device emulator is not configured.");

        return;
    };

    let connection_name = definition
        .connection_name_by_id(emulator.connection_id())
        .unwrap_or("<unknown>");

    egui::Grid::new("settings_device_emulator")
        .num_columns(2)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            ui.label("Connection");
            ui.label(format!(
                "{} ({})",
                connection_name,
                emulator.connection_id(),
            ));
            ui.end_row();

            ui.label("Port");
            ui.monospace(emulator.port_name());
            ui.end_row();

            ui.label("Script");
            ui.monospace(emulator.script_path().display().to_string());
            ui.end_row();

            ui.label("Resolved script");
            ui.monospace(
                paths
                    .resolve_profile(emulator.script_path())
                    .display()
                    .to_string(),
            );
            ui.end_row();
        });
}

fn format_duration(duration: Duration) -> String {
    format!("{:.3} s", duration.as_secs_f64())
}

fn show_validation(ui: &mut egui::Ui, model: &mut SettingsModel, runtime: &ApplicationRuntime) {
    ui.horizontal(|ui| {
        if ui.button("Open startup.lua").clicked() {
            model.open_startup_file(runtime);
        }

        if ui.button("Validate startup.lua").clicked() {
            model.validate(runtime);
        }

        if ui.button("Reload startup.lua").clicked() {
            model.begin_reload_confirmation();
        }

        match model.validation() {
            SettingsValidation::NotChecked => {
                ui.weak("Configuration has not been checked.");
            }

            SettingsValidation::Valid => {
                ui.colored_label(
                    egui::Color32::from_rgb(0, 140, 0),
                    "Configuration is valid.",
                );
            }

            SettingsValidation::Invalid(_) => {
                ui.colored_label(
                    egui::Color32::from_rgb(190, 30, 30),
                    "Configuration is invalid.",
                );
            }
        }
    });

    if model.reload_confirmation_open() {
        ui.add_space(8.0);

        ui.group(|ui| {
            ui.colored_label(
                egui::Color32::from_rgb(190, 130, 0),
                "Reloading will clear all current series \
                 and plot history.",
            );

            ui.horizontal(|ui| {
                if ui.button("Reload now").clicked() {
                    model.confirm_reload();
                }

                if ui.button("Cancel").clicked() {
                    model.cancel_reload();
                }
            });
        });
    }

    match model.reload_status() {
        SettingsReloadStatus::NotReloaded => {}

        SettingsReloadStatus::Succeeded => {
            ui.add_space(4.0);

            ui.colored_label(
                egui::Color32::from_rgb(0, 140, 0),
                "Configuration reloaded successfully.",
            );
        }

        SettingsReloadStatus::Failed(error) => {
            ui.add_space(4.0);

            ui.colored_label(egui::Color32::from_rgb(190, 30, 30), error);
        }
    }

    if let Some(error) = model.open_error() {
        ui.add_space(4.0);

        ui.colored_label(egui::Color32::from_rgb(190, 30, 30), error);
    }

    if let SettingsValidation::Invalid(error) = model.validation() {
        ui.add_space(4.0);

        ui.colored_label(egui::Color32::from_rgb(190, 30, 30), error);
    }
}
