use std::path::{Path, PathBuf};

use crate::{
    app_log::LogHandle,
    components::serial_settings_model::SerialSettings,
    device_emulator_handle::{DeviceEmulatorHandle, DeviceEmulatorPortConfig},
    device_model::DeviceModelSource,
};

pub struct DeviceEmulatorModel {
    selected_port: Option<String>,
    handle: Option<DeviceEmulatorHandle>,
    error: Option<String>,
    log: LogHandle,
    model_source: DeviceModelSource,
}

impl DeviceEmulatorModel {
    pub fn new(configured_port: &str, log: LogHandle) -> Self {
        Self {
            selected_port: if configured_port.is_empty() {
                None
            } else {
                Some(configured_port.to_owned())
            },

            handle: None,
            error: None,
            log,
            model_source: DeviceModelSource::BuiltIn,
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

        let conflicts_with_client = self.selected_port.as_deref() == client_port;

        if self.selected_port.is_none() || conflicts_with_client {
            self.selected_port = ports
                .iter()
                .find(|port| Some(port.as_str()) != client_port)
                .cloned();

            self.error = None;
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

        let model_source = self.model_source.clone();

        let model_description = match &model_source {
            DeviceModelSource::BuiltIn => "built-in model".to_owned(),

            DeviceModelSource::LuaScript(path) => {
                format!("Lua model '{}'", path.display())
            }
        };

        match DeviceEmulatorHandle::start(config, model_source) {
            Ok(handle) => {
                self.handle = Some(handle);
                self.error = None;

                self.log.info(format!(
                    "Device emulator started on {port_name} \
                     using {model_description}.",
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

        let port_name = self.selected_port.clone();

        match handle.stop() {
            Ok(()) => {
                self.error = None;

                match port_name {
                    Some(port_name) => {
                        self.log.info(format!(
                            "Device emulator stopped on \
                             {port_name}.",
                        ));
                    }

                    None => {
                        self.log.info("Device emulator stopped.");
                    }
                }
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

        let port_name = self.selected_port.clone();

        match handle.stop() {
            Ok(()) => match port_name {
                Some(port_name) => {
                    self.log.info(format!(
                        "Device emulator stopped on \
                             {port_name}.",
                    ));
                }

                None => {
                    self.log.info("Device emulator stopped.");
                }
            },

            Err(error) => {
                let location = port_name
                    .as_deref()
                    .map_or(String::new(), |port| format!(" on {port}"));

                self.report_error(format!(
                    "Device emulator{location} stopped \
                     with an error: {error}",
                ));
            }
        }
    }

    fn report_error(&mut self, message: impl Into<String>) {
        let message = message.into();

        self.log.error(message.clone());
        self.error = Some(message);
    }

    pub fn script_path(&self) -> Option<&Path> {
        match &self.model_source {
            DeviceModelSource::BuiltIn => None,

            DeviceModelSource::LuaScript(path) => Some(path),
        }
    }

    pub fn set_script_path(&mut self, path: PathBuf) {
        if self.is_running() {
            return;
        }

        self.model_source = DeviceModelSource::LuaScript(path);

        self.error = None;
    }

    pub fn use_built_in_model(&mut self) {
        if self.is_running() {
            return;
        }

        self.model_source = DeviceModelSource::BuiltIn;
        self.error = None;
    }
}
