use eframe::egui;
use serialport::{DataBits, FlowControl, Parity, StopBits};

use super::serial_settings_model::SerialSettingsModel;
use crate::worker::WorkerHandle;

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

pub fn show(ui: &mut egui::Ui, model: &mut SerialSettingsModel, worker_handle: &WorkerHandle) {
    ui.horizontal(|ui| {
        ui.label("COM port:");

        let mut selected_port = model.selected_port().map(str::to_owned);

        let selected_text = selected_port.as_deref().unwrap_or("No ports found");

        egui::ComboBox::from_id_salt("serial_port_selector")
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for port in model.ports() {
                    ui.selectable_value(&mut selected_port, Some(port.clone()), port);
                }
            });

        if selected_port.as_deref() != model.selected_port() {
            model.set_selected_port(selected_port);
        }

        if ui.button("Refresh ports").clicked() {
            model.refresh_ports();
        }

        if ui.button("COM settings").clicked() {
            model.open_settings();
        }

        if ui.button("Test connection").clicked() {
            model.test_connection(worker_handle);
        }
    });

    if let Some(error) = model.error() {
        ui.colored_label(egui::Color32::RED, error);
    }

    show_settings_window(ui.ctx(), model, worker_handle);
}

fn show_settings_window(
    context: &egui::Context,
    model: &mut SerialSettingsModel,
    worker_handle: &WorkerHandle,
) {
    let mut open = model.settings_open();

    if !open {
        return;
    }

    egui::Window::new("COM settings")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(320.0)
        .show(context, |ui| {
            let settings = model.settings_mut();

            egui::Grid::new("com_settings_grid")
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
                                ui.selectable_value(
                                    &mut settings.parity,
                                    parity,
                                    parity_label(parity),
                                );
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

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Test command:");

                if ui.button("sin").clicked() {
                    model.test_command(worker_handle, "sin");
                }

                if ui.button("noise").clicked() {
                    model.test_command(worker_handle, "noise");
                }
            });

            ui.small(
                "Each test opens the port, sends one command, \
                                    reads one f64 response and closes the port.",
            );

            ui.separator();

            ui.small("Settings will be applied when the COM port is opened.");
        });

    model.set_settings_open(open);
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
