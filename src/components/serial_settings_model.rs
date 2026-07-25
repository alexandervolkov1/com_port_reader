use serialport::{DataBits, FlowControl, Parity, StopBits};

use crate::{
    app_config::{
        SerialFlowControl as ConfigFlowControl, SerialParity as ConfigParity,
        SerialPortSettings as ConfigSerialSettings,
    },
    serial_connection::{SerialConfigStore, SerialPortConfig},
    worker::WorkerHandle,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SerialSettings {
    pub baud_rate: u32,
    pub data_bits: DataBits,
    pub parity: Parity,
    pub stop_bits: StopBits,
    pub flow_control: FlowControl,
    pub timeout_ms: u64,
}

impl From<&ConfigSerialSettings> for SerialSettings {
    fn from(config: &ConfigSerialSettings) -> Self {
        Self {
            baud_rate: config.baud_rate,

            data_bits: match config.data_bits {
                5 => DataBits::Five,
                6 => DataBits::Six,
                7 => DataBits::Seven,
                8 => DataBits::Eight,

                _ => DataBits::Eight,
            },

            parity: match config.parity {
                ConfigParity::None => Parity::None,
                ConfigParity::Even => Parity::Even,
                ConfigParity::Odd => Parity::Odd,
            },

            stop_bits: match config.stop_bits {
                2 => StopBits::Two,
                _ => StopBits::One,
            },

            flow_control: match config.flow_control {
                ConfigFlowControl::None => FlowControl::None,

                ConfigFlowControl::Software => FlowControl::Software,

                ConfigFlowControl::Hardware => FlowControl::Hardware,
            },

            timeout_ms: config.timeout_ms,
        }
    }
}

impl Default for SerialSettings {
    fn default() -> Self {
        Self {
            baud_rate: 9_600,
            data_bits: DataBits::Eight,
            parity: Parity::None,
            stop_bits: StopBits::One,
            flow_control: FlowControl::None,
            timeout_ms: 250,
        }
    }
}

pub struct SerialSettingsModel {
    ports: Vec<String>,
    selected_port: Option<String>,
    settings: SerialSettings,
    settings_open: bool,
    error: Option<String>,
    config_store: SerialConfigStore,
}

impl SerialSettingsModel {
    pub fn new(config_store: SerialConfigStore, config: &ConfigSerialSettings) -> Self {
        let selected_port = configured_port(&config.main_port);

        let mut model = Self {
            ports: Vec::new(),
            selected_port,
            settings: SerialSettings::from(config),
            settings_open: false,
            error: None,
            config_store,
        };

        model.refresh_ports();

        model
    }

    pub fn refresh_ports(&mut self) {
        match serialport::available_ports() {
            Ok(ports) => {
                self.ports = ports.into_iter().map(|port| port.port_name).collect();

                self.ports.sort();
                self.ports.dedup();

                if self.selected_port.is_none() {
                    self.selected_port = self.ports.first().cloned();
                }

                self.error = None;
            }

            Err(error) => {
                self.ports.clear();

                self.error = Some(format!("Failed to enumerate COM ports: {error}",));
            }
        }

        self.publish_config();
    }

    pub fn ports(&self) -> &[String] {
        &self.ports
    }

    pub fn selected_port(&self) -> Option<&str> {
        self.selected_port.as_deref()
    }

    pub fn set_selected_port(&mut self, selected_port: Option<String>) {
        self.selected_port = selected_port;
        self.publish_config();
    }

    pub fn settings(&self) -> SerialSettings {
        self.settings
    }

    pub fn settings_mut(&mut self) -> &mut SerialSettings {
        &mut self.settings
    }

    pub fn settings_open(&self) -> bool {
        self.settings_open
    }

    pub fn open_settings(&mut self) {
        self.settings_open = true;
    }

    pub fn set_settings_open(&mut self, open: bool) {
        self.settings_open = open;
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn test_connection(&mut self, worker_handle: &WorkerHandle) {
        let Some(config) = self.serial_config() else {
            self.error = Some("Select a COM port first.".to_owned());

            return;
        };

        match worker_handle.test_serial_port(config) {
            Ok(()) => {
                self.error = None;
            }

            Err(error) => {
                self.error = Some(error.to_string());
            }
        }
    }

    pub fn test_command(&mut self, worker_handle: &WorkerHandle, command: &str) {
        let Some(config) = self.serial_config() else {
            self.error = Some("Select a COM port first.".to_owned());

            return;
        };

        match worker_handle.test_serial_command(config, command.to_owned()) {
            Ok(()) => {
                self.error = None;
            }

            Err(error) => {
                self.error = Some(error.to_string());
            }
        }
    }

    pub fn publish_config(&self) {
        self.config_store.set(self.serial_config());
    }

    fn serial_config(&self) -> Option<SerialPortConfig> {
        let port_name = self.selected_port.clone()?;
        let settings = self.settings;

        Some(SerialPortConfig::new(
            port_name,
            settings.baud_rate,
            settings.data_bits,
            settings.parity,
            settings.stop_bits,
            settings.flow_control,
            settings.timeout_ms,
        ))
    }

    pub fn write_to_config(&self, config: &mut ConfigSerialSettings) {
        config.main_port = self.selected_port.clone().unwrap_or_default();

        config.baud_rate = self.settings.baud_rate;

        config.data_bits = match self.settings.data_bits {
            DataBits::Five => 5,
            DataBits::Six => 6,
            DataBits::Seven => 7,
            DataBits::Eight => 8,
        };

        config.parity = match self.settings.parity {
            Parity::None => ConfigParity::None,
            Parity::Even => ConfigParity::Even,
            Parity::Odd => ConfigParity::Odd,
        };

        config.stop_bits = match self.settings.stop_bits {
            StopBits::One => 1,
            StopBits::Two => 2,
        };

        config.flow_control = match self.settings.flow_control {
            FlowControl::None => ConfigFlowControl::None,

            FlowControl::Software => ConfigFlowControl::Software,

            FlowControl::Hardware => ConfigFlowControl::Hardware,
        };

        config.timeout_ms = self.settings.timeout_ms;
    }
}

impl Default for SerialSettingsModel {
    fn default() -> Self {
        Self::new(SerialConfigStore::new(), &ConfigSerialSettings::default())
    }
}

fn configured_port(port: &str) -> Option<String> {
    if port.is_empty() {
        None
    } else {
        Some(port.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use serialport::{DataBits, FlowControl, Parity, StopBits};

    use super::SerialSettings;
    use crate::app_config::{SerialFlowControl, SerialParity, SerialPortSettings};

    #[test]
    fn converts_configuration_to_runtime_settings() {
        let config = SerialPortSettings {
            main_port: "COM3".to_owned(),
            emulator_port: "COM4".to_owned(),
            baud_rate: 115_200,
            data_bits: 7,
            parity: SerialParity::Even,
            stop_bits: 2,
            flow_control: SerialFlowControl::Hardware,
            timeout_ms: 500,
        };

        let settings = SerialSettings::from(&config);

        assert_eq!(settings.baud_rate, 115_200);
        assert_eq!(settings.data_bits, DataBits::Seven);
        assert_eq!(settings.parity, Parity::Even);
        assert_eq!(settings.stop_bits, StopBits::Two);
        assert_eq!(settings.flow_control, FlowControl::Hardware,);
        assert_eq!(settings.timeout_ms, 500);
    }

    #[test]
    fn empty_configured_port_is_not_selected() {
        assert_eq!(super::configured_port(""), None);
    }

    #[test]
    fn preserves_configured_port() {
        assert_eq!(super::configured_port("COM3"), Some("COM3".to_owned()),);
    }
}
