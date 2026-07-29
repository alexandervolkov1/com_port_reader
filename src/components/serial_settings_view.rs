use eframe::egui;
use rfd::FileDialog;
use serialport::{DataBits, FlowControl, Parity, StopBits};

use super::{
    device_emulator_model::DeviceEmulatorModel, serial_settings_model::SerialSettingsModel,
};

use crate::{
    app_config::{AppConfig, ApplicationSettings, CONFIG_PATH},
    app_log::LogHandle,
    worker::WorkerHandle,
};

const BAUD_RATES: &[u32] = &[1_200, 2_400, 4_800, 9_600, 19_200, 38_400, 57_600, 115_200];

const DATA_BITS: &[DataBits] = &[
    DataBits::Five,
    DataBits::Six,
    DataBits::Seven,
    DataBits::Eight,
];

const PARITIES: &[Parity] = &[Parity::None, Parity::Even, Parity::Odd];

const STOP_BITS: &[StopBits] = &[StopBits::One, StopBits::Two];

const FLOW_CONTROLS: &[FlowControl] = &[
    FlowControl::None,
    FlowControl::Software,
    FlowControl::Hardware,
];

pub fn show_menu_button(ui: &mut egui::Ui, model: &mut SerialSettingsModel) {
    ui.menu_button("Settings", |ui| {
        if ui.button("Open settings...").clicked() {
            model.open_settings();
            ui.close();
        }
    });
}

pub fn show_window(
    context: &egui::Context,
    model: &mut SerialSettingsModel,
    emulator: &mut DeviceEmulatorModel,
    worker_handle: &WorkerHandle,
    acquisition_running: bool,
    config: &mut AppConfig,
    log: &LogHandle,
) {
    let mut open = model.settings_open();
    let mut save_requested = false;

    if !open {
        return;
    }

    let serial_settings_locked = acquisition_running || emulator.is_running();

    egui::Window::new("Settings")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .show(context, |ui| {
            ui.heading("Application");

            show_application_settings(ui, &mut config.application);

            ui.small(
                "FPS and plot point limit apply while editing. \
                 Poll interval applies when settings are saved.",
            );

            ui.separator();

            ui.heading("COM ports");

            if serial_settings_locked {
                ui.colored_label(
                    egui::Color32::from_rgb(190, 130, 0),
                    "Stop acquisition and emulator to change \
                     serial settings.",
                );
            }

            show_main_port_controls(ui, model, worker_handle, !serial_settings_locked);
            emulator.synchronize_ports(model.ports(), model.selected_port());

            if let Some(error) = model.error() {
                ui.colored_label(egui::Color32::RED, error);
            }

            ui.separator();

            ui.heading("Serial line");

            ui.add_enabled_ui(!serial_settings_locked, |ui| {
                show_line_settings(ui, model);
            });

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!serial_settings_locked, egui::Button::new("Test walk"))
                    .clicked()
                {
                    model.test_command(worker_handle, "walk");
                }
            });

            ui.separator();

            show_emulator_controls(ui, model, emulator, acquisition_running);

            ui.separator();

            if ui.button("Save to config.toml").clicked() {
                save_requested = true;
            }
        });

    model.publish_config();
    model.set_settings_open(open);

    if save_requested {
        model.write_to_config(&mut config.serial);

        config.serial.emulator_port = emulator.selected_port().unwrap_or_default().to_owned();

        let apply_result = worker_handle.set_poll_interval(config.application.poll_interval());

        let save_result = config.save(CONFIG_PATH);

        match apply_result {
            Ok(()) => {
                log.info(format!(
                    "Poll interval changed to {} ms.",
                    config.application.poll_interval_ms,
                ));
            }

            Err(error) => {
                log.error(format!(
                    "Failed to apply poll interval: \
                     {error}",
                ));
            }
        }

        match save_result {
            Ok(()) => {
                log.info(format!("Settings saved to '{CONFIG_PATH}'.",));
            }

            Err(error) => {
                log.error(error);
            }
        }
    }
}

fn show_main_port_controls(
    ui: &mut egui::Ui,
    model: &mut SerialSettingsModel,
    worker_handle: &WorkerHandle,
    settings_enabled: bool,
) {
    let mut selected_port = model.selected_port().map(str::to_owned);

    let selected_text = selected_port
        .clone()
        .unwrap_or_else(|| "No port selected".to_owned());

    ui.horizontal(|ui| {
        ui.label("Application port:");

        ui.add_enabled_ui(settings_enabled, |ui| {
            egui::ComboBox::from_id_salt("serial_port_selector")
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for port in model.ports() {
                        ui.selectable_value(&mut selected_port, Some(port.clone()), port);
                    }
                });
        });

        if ui.button("Refresh").clicked() {
            model.refresh_ports();
        }

        if ui
            .add_enabled(settings_enabled, egui::Button::new("Test connection"))
            .clicked()
        {
            model.test_connection(worker_handle);
        }
    });

    if settings_enabled && selected_port.as_deref() != model.selected_port() {
        model.set_selected_port(selected_port);
    }
}

fn show_line_settings(ui: &mut egui::Ui, model: &mut SerialSettingsModel) {
    let settings = model.settings_mut();

    egui::Grid::new("serial_line_settings_grid")
        .num_columns(2)
        .spacing([20.0, 10.0])
        .show(ui, |ui| {
            ui.label("Baud rate:");

            egui::ComboBox::from_id_salt("com_baud_rate")
                .selected_text(settings.baud_rate.to_string())
                .show_ui(ui, |ui| {
                    for &baud_rate in BAUD_RATES {
                        ui.selectable_value(
                            &mut settings.baud_rate,
                            baud_rate,
                            baud_rate.to_string(),
                        );
                    }
                });

            ui.end_row();

            ui.label("Data bits:");

            egui::ComboBox::from_id_salt("com_data_bits")
                .selected_text(data_bits_label(settings.data_bits))
                .show_ui(ui, |ui| {
                    for &data_bits in DATA_BITS {
                        ui.selectable_value(
                            &mut settings.data_bits,
                            data_bits,
                            data_bits_label(data_bits),
                        );
                    }
                });

            ui.end_row();

            ui.label("Parity:");

            egui::ComboBox::from_id_salt("com_parity")
                .selected_text(parity_label(settings.parity))
                .show_ui(ui, |ui| {
                    for &parity in PARITIES {
                        ui.selectable_value(&mut settings.parity, parity, parity_label(parity));
                    }
                });

            ui.end_row();

            ui.label("Stop bits:");

            egui::ComboBox::from_id_salt("com_stop_bits")
                .selected_text(stop_bits_label(settings.stop_bits))
                .show_ui(ui, |ui| {
                    for &stop_bits in STOP_BITS {
                        ui.selectable_value(
                            &mut settings.stop_bits,
                            stop_bits,
                            stop_bits_label(stop_bits),
                        );
                    }
                });

            ui.end_row();

            ui.label("Flow control:");

            egui::ComboBox::from_id_salt("com_flow_control")
                .selected_text(flow_control_label(settings.flow_control))
                .show_ui(ui, |ui| {
                    for &flow_control in FLOW_CONTROLS {
                        ui.selectable_value(
                            &mut settings.flow_control,
                            flow_control,
                            flow_control_label(flow_control),
                        );
                    }
                });

            ui.end_row();

            ui.label("Read timeout:");

            ui.add(
                egui::DragValue::new(&mut settings.timeout_ms)
                    .range(1..=60_000)
                    .speed(10.0)
                    .suffix(" ms"),
            );

            ui.end_row();
        });
}

fn show_emulator_controls(
    ui: &mut egui::Ui,
    serial: &SerialSettingsModel,
    emulator: &mut DeviceEmulatorModel,
    acquisition_running: bool,
) {
    ui.heading("Device emulator");

    ui.horizontal(|ui| {
        let port_selection_enabled = !acquisition_running && !emulator.is_running();

        ui.label("Emulator port:");

        let mut selected_port = emulator.selected_port().map(str::to_owned);

        let selected_text = selected_port
            .clone()
            .unwrap_or_else(|| "No port selected".to_owned());

        ui.add_enabled_ui(port_selection_enabled, |ui| {
            egui::ComboBox::from_id_salt("device_emulator_port")
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for port in serial.ports() {
                        if Some(port.as_str()) == serial.selected_port() {
                            continue;
                        }

                        ui.selectable_value(&mut selected_port, Some(port.clone()), port);
                    }
                });
        });

        if selected_port.as_deref() != emulator.selected_port() {
            emulator.set_selected_port(selected_port);
        }
    });

    let model_selection_enabled = !emulator.is_running();

    let using_lua_model = emulator.script_path().is_some();

    let model_name = emulator
        .script_path()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Built-in random walk".to_owned());

    let model_tooltip = emulator
        .script_path()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "Built-in Rust random-walk model".to_owned());

    ui.horizontal(|ui| {
        ui.label("Device model:");

        ui.label(model_name).on_hover_text(model_tooltip);

        if ui
            .add_enabled(model_selection_enabled, egui::Button::new("Choose Lua..."))
            .clicked()
            && let Some(path) = FileDialog::new()
                .set_title("Select emulator Lua model")
                .set_directory("emulator_scripts")
                .add_filter("Lua scripts", &["lua"])
                .pick_file()
        {
            emulator.set_script_path(path);
        }

        if ui
            .add_enabled(
                model_selection_enabled && using_lua_model,
                egui::Button::new("Use built-in"),
            )
            .clicked()
        {
            emulator.use_built_in_model();
        }
    });

    ui.horizontal(|ui| {
        let can_start = emulator.can_start(serial.selected_port());
        if ui
            .add_enabled(can_start, egui::Button::new("Start emulator"))
            .clicked()
        {
            emulator.start(serial.settings(), serial.selected_port());
        }

        if ui
            .add_enabled(emulator.is_running(), egui::Button::new("Stop emulator"))
            .clicked()
        {
            emulator.stop();
        }

        if emulator.is_running() {
            ui.colored_label(egui::Color32::from_rgb(0, 150, 0), "● Running");
        } else {
            ui.colored_label(egui::Color32::GRAY, "■ Stopped");
        }
    });

    if let Some(error) = emulator.error() {
        ui.colored_label(egui::Color32::RED, error);
    }
}

fn data_bits_label(value: DataBits) -> &'static str {
    match value {
        DataBits::Five => "5",
        DataBits::Six => "6",
        DataBits::Seven => "7",
        DataBits::Eight => "8",
    }
}

fn parity_label(value: Parity) -> &'static str {
    match value {
        Parity::None => "None",
        Parity::Even => "Even",
        Parity::Odd => "Odd",
    }
}

fn stop_bits_label(value: StopBits) -> &'static str {
    match value {
        StopBits::One => "1",
        StopBits::Two => "2",
    }
}

fn flow_control_label(value: FlowControl) -> &'static str {
    match value {
        FlowControl::None => "None",

        FlowControl::Software => "Software (XON/XOFF)",

        FlowControl::Hardware => "Hardware (RTS/CTS)",
    }
}

fn show_application_settings(ui: &mut egui::Ui, settings: &mut ApplicationSettings) {
    egui::Grid::new("application_settings_grid")
        .num_columns(2)
        .spacing([20.0, 10.0])
        .show(ui, |ui| {
            ui.label("FPS:");

            ui.add(
                egui::DragValue::new(&mut settings.fps)
                    .range(1..=240)
                    .speed(1.0),
            );

            ui.end_row();

            ui.label("Poll interval:");

            ui.add(
                egui::DragValue::new(&mut settings.poll_interval_ms)
                    .range(1..=86_400_000)
                    .speed(10.0)
                    .suffix(" ms"),
            );

            ui.end_row();

            ui.label("Plot points per series:");

            ui.add(
                egui::DragValue::new(&mut settings.max_plot_points_per_series)
                    .range(4..=100_000)
                    .speed(100.0),
            );

            ui.end_row();
        });
}
