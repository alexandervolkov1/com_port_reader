use crate::{
    app_log::LogHandle,
    components::serial_settings_model::SerialSettings,
    device_emulator_handle::{DeviceEmulatorHandle, DeviceEmulatorPortConfig},
};

pub struct DeviceEmulatorModel {
    selected_port: Option<String>,
    handle: Option<DeviceEmulatorHandle>,
    error: Option<String>,
    log: LogHandle,
}

impl DeviceEmulatorModel {
    pub fn new(log: LogHandle) -> Self {
        Self {
            selected_port: None,
            handle: None,
            error: None,
            log,
        }
    }

    pub fn selected_port(&self) -> Option<&str> {
        self.selected_port.as_deref()
    }

    pub fn set_selected_port(&mut self, selected_port: Option<String>) {
        if self.is_running() {
            return;
        }

        self.selected_port = selected_port;
        self.error = None;
    }

    pub fn synchronize_ports(&mut self, ports: &[String], client_port: Option<&str>) {
        if self.is_running() {
            return;
        }

        let selection_is_available = self.selected_port.as_deref().is_some_and(|selected_port| {
            ports.iter().any(|port| port == selected_port) && Some(selected_port) != client_port
        });

        if !selection_is_available {
            self.selected_port = ports
                .iter()
                .find(|port| Some(port.as_str()) != client_port)
                .cloned();
        }
    }

    pub fn is_running(&self) -> bool {
        self.handle
            .as_ref()
            .is_some_and(DeviceEmulatorHandle::is_running)
    }

    pub fn can_start(&self, client_port: Option<&str>) -> bool {
        !self.is_running()
            && self
                .selected_port()
                .is_some_and(|port| Some(port) != client_port)
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn start(&mut self, settings: SerialSettings, client_port: Option<&str>) {
        self.poll();

        if self.handle.is_some() {
            return;
        }

        let Some(port_name) = self.selected_port.clone() else {
            self.report_error("Select an emulator COM port first.");

            return;
        };

        if Some(port_name.as_str()) == client_port {
            self.report_error(
                "The application and emulator must use \
                 different COM ports.",
            );

            return;
        }

        let config = DeviceEmulatorPortConfig {
            port_name: port_name.clone(),
            baud_rate: settings.baud_rate,
            data_bits: settings.data_bits,
            parity: settings.parity,
            stop_bits: settings.stop_bits,
            flow_control: settings.flow_control,
        };

        match DeviceEmulatorHandle::start(config) {
            Ok(handle) => {
                self.handle = Some(handle);
                self.error = None;

                self.log.info(format!(
                    "Device emulator started on \
                     {port_name}.",
                ));
            }

            Err(error) => {
                self.report_error(format!(
                    "Failed to start device emulator on \
                     {port_name}: {error}",
                ));
            }
        }
    }

    pub fn stop(&mut self) {
        let Some(mut handle) = self.handle.take() else {
            return;
        };

        match handle.stop() {
            Ok(()) => {
                self.error = None;

                self.log.info("Device emulator stopped.");
            }

            Err(error) => {
                self.report_error(format!("Device emulator failed: {error}",));
            }
        }
    }

    pub fn poll(&mut self) {
        let finished = self
            .handle
            .as_ref()
            .is_some_and(|handle| !handle.is_running());

        if !finished {
            return;
        }

        let Some(mut handle) = self.handle.take() else {
            return;
        };

        match handle.stop() {
            Ok(()) => {
                self.log.info("Device emulator stopped.");
            }

            Err(error) => {
                self.report_error(format!(
                    "Device emulator stopped with an \
                     error: {error}",
                ));
            }
        }
    }

    fn report_error(&mut self, message: impl Into<String>) {
        let message = message.into();

        self.log.error(message.clone());
        self.error = Some(message);
    }
}
