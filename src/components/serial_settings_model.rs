use serialport::{DataBits, FlowControl, Parity, StopBits};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SerialSettings {
    pub baud_rate: u32,
    pub data_bits: DataBits,
    pub parity: Parity,
    pub stop_bits: StopBits,
    pub flow_control: FlowControl,
    pub timeout_ms: u64,
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
}

impl SerialSettingsModel {
    pub fn new() -> Self {
        let mut model = Self {
            ports: Vec::new(),
            selected_port: None,
            settings: SerialSettings::default(),
            settings_open: false,
            error: None,
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

                let selection_is_available = self
                    .selected_port
                    .as_ref()
                    .is_some_and(|selected| self.ports.contains(selected));

                if !selection_is_available {
                    self.selected_port = self.ports.first().cloned();
                }

                self.error = None;
            }

            Err(error) => {
                self.ports.clear();
                self.selected_port = None;

                self.error = Some(format!("Failed to enumerate COM ports: {error}",));
            }
        }
    }

    pub fn ports(&self) -> &[String] {
        &self.ports
    }

    pub fn selected_port(&self) -> Option<&str> {
        self.selected_port.as_deref()
    }

    pub fn set_selected_port(&mut self, selected_port: Option<String>) {
        self.selected_port = selected_port;
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
}

impl Default for SerialSettingsModel {
    fn default() -> Self {
        Self::new()
    }
}
