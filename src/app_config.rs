use std::{fs, path::Path, time::Duration};

use serde::{Deserialize, Serialize};
use serialport::{DataBits, FlowControl, Parity, StopBits};

use crate::{
    application_definition::{
        ApplicationDefinition, ApplicationDefinitionError, RuntimeDefinition,
        SerialConnectionDefinition,
    },
    connection::ConnectionId,
    serial_connection::SerialPortConfig,
};

pub const CONFIG_PATH: &str = "config.toml";

const DEFAULT_FPS: u32 = 30;
const DEFAULT_POLL_INTERVAL_MS: u64 = 1_000;
const DEFAULT_PLOT_WINDOW_SECONDS: u64 = 3_600;
const DEFAULT_MAX_PLOT_POINTS_PER_SERIES: usize = 4_000;
const MIN_PLOT_WINDOW_SECONDS: u64 = 1;
const MAX_PLOT_WINDOW_SECONDS: u64 = 14 * 24 * 60 * 60;
const DEFAULT_BAUD_RATE: u32 = 9_600;
const DEFAULT_DATA_BITS: u8 = 8;
const DEFAULT_STOP_BITS: u8 = 1;
const DEFAULT_TIMEOUT_MS: u64 = 250;
const MIN_PLOT_POINTS_PER_SERIES: usize = 4;
const MAX_PLOT_POINTS_PER_SERIES: usize = 100_000;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub application: ApplicationSettings,
    pub serial: SerialPortSettings,
    pub emulator: EmulatorSettings,
}

impl AppConfig {
    pub fn load_or_default(path: impl AsRef<Path>) -> (Self, Option<String>) {
        let path = path.as_ref();

        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,

            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return (Self::default(), None);
            }

            Err(error) => {
                return (
                    Self::default(),
                    Some(format!(
                        "Failed to read configuration \
                         '{}': {error}. Defaults will be used.",
                        path.display(),
                    )),
                );
            }
        };

        match Self::parse(&contents) {
            Ok(config) => (config, None),

            Err(error) => (
                Self::default(),
                Some(format!(
                    "Failed to load configuration \
                     '{}': {error}. Defaults will be used.",
                    path.display(),
                )),
            ),
        }
    }

    fn parse(contents: &str) -> Result<Self, String> {
        let config =
            toml::from_str::<Self>(contents).map_err(|error| format!("invalid TOML: {error}"))?;

        config.validate()?;

        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        self.application.validate()?;
        self.serial.validate()?;

        Ok(())
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), String> {
        self.validate()?;

        let contents = toml::to_string_pretty(self).map_err(|error| {
            format!(
                "Failed to serialize configuration: \
                     {error}",
            )
        })?;

        let path = path.as_ref();

        fs::write(path, contents).map_err(|error| {
            format!(
                "Failed to write configuration '{}': \
                 {error}",
                path.display(),
            )
        })
    }
}

impl TryFrom<&AppConfig> for ApplicationDefinition {
    type Error = ApplicationDefinitionError;

    fn try_from(config: &AppConfig) -> Result<Self, Self::Error> {
        config.validate().map_err(ApplicationDefinitionError::new)?;

        let runtime = RuntimeDefinition::new(
            config.application.fps,
            config.application.poll_interval(),
            Duration::from_secs(config.application.plot_window_seconds),
            config.application.max_plot_points_per_series,
        )?;

        let mut definition = ApplicationDefinition::new(runtime);

        if config.serial.main_port.is_empty() {
            return Ok(definition);
        }

        let data_bits = match config.serial.data_bits {
            5 => DataBits::Five,
            6 => DataBits::Six,
            7 => DataBits::Seven,
            8 => DataBits::Eight,

            _ => {
                return Err(ApplicationDefinitionError::new(
                    "Serial data bits must be \
                         5, 6, 7 or 8",
                ));
            }
        };

        let parity = match config.serial.parity {
            SerialParity::None => Parity::None,
            SerialParity::Even => Parity::Even,
            SerialParity::Odd => Parity::Odd,
        };

        let stop_bits = match config.serial.stop_bits {
            1 => StopBits::One,
            2 => StopBits::Two,

            _ => {
                return Err(ApplicationDefinitionError::new(
                    "Serial stop bits must be \
                         1 or 2",
                ));
            }
        };

        let flow_control = match config.serial.flow_control {
            SerialFlowControl::None => FlowControl::None,

            SerialFlowControl::Software => FlowControl::Software,

            SerialFlowControl::Hardware => FlowControl::Hardware,
        };

        let serial_config = SerialPortConfig::new(
            config.serial.main_port.clone(),
            config.serial.baud_rate,
            data_bits,
            parity,
            stop_bits,
            flow_control,
            config.serial.timeout_ms,
        );

        let connection =
            SerialConnectionDefinition::new(ConnectionId::PRIMARY, "primary", serial_config)?;

        definition.add_serial_connection(connection)?;

        Ok(definition)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ApplicationSettings {
    pub fps: u32,
    pub poll_interval_ms: u64,
    pub plot_window_seconds: u64,
    pub max_plot_points_per_series: usize,
}

impl ApplicationSettings {
    pub fn repaint_interval(&self) -> Duration {
        Duration::from_secs_f64(1.0 / f64::from(self.fps))
    }

    pub const fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.poll_interval_ms)
    }

    fn validate(&self) -> Result<(), String> {
        if !(1..=240).contains(&self.fps) {
            return Err("application.fps must be between 1 and 240".to_owned());
        }

        if self.poll_interval_ms == 0 {
            return Err("application.poll_interval_ms must be \
                 greater than zero"
                .to_owned());
        }

        if !(MIN_PLOT_WINDOW_SECONDS..=MAX_PLOT_WINDOW_SECONDS).contains(&self.plot_window_seconds)
        {
            return Err(format!(
                "application.plot_window_seconds must be between \
                 {MIN_PLOT_WINDOW_SECONDS} and \
                 {MAX_PLOT_WINDOW_SECONDS}",
            ));
        }

        if !(MIN_PLOT_POINTS_PER_SERIES..=MAX_PLOT_POINTS_PER_SERIES)
            .contains(&self.max_plot_points_per_series)
        {
            return Err(format!(
                "application.max_plot_points_per_series \
                 must be between \
                 {MIN_PLOT_POINTS_PER_SERIES} and \
                 {MAX_PLOT_POINTS_PER_SERIES}",
            ));
        }

        Ok(())
    }
}

impl Default for ApplicationSettings {
    fn default() -> Self {
        Self {
            fps: DEFAULT_FPS,
            poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
            plot_window_seconds: DEFAULT_PLOT_WINDOW_SECONDS,
            max_plot_points_per_series: DEFAULT_MAX_PLOT_POINTS_PER_SERIES,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EmulatorSettings {
    pub script_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SerialPortSettings {
    pub main_port: String,
    pub emulator_port: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub parity: SerialParity,
    pub stop_bits: u8,
    pub flow_control: SerialFlowControl,
    pub timeout_ms: u64,
}

impl SerialPortSettings {
    fn validate(&self) -> Result<(), String> {
        validate_port_name("serial.main_port", &self.main_port)?;

        validate_port_name("serial.emulator_port", &self.emulator_port)?;

        if !self.main_port.is_empty() && self.main_port.eq_ignore_ascii_case(&self.emulator_port) {
            return Err("serial.main_port and serial.emulator_port \
                 must be different"
                .to_owned());
        }

        if self.baud_rate == 0 {
            return Err("serial.baud_rate must be greater than zero".to_owned());
        }

        if !matches!(self.data_bits, 5..=8) {
            return Err("serial.data_bits must be 5, 6, 7 or 8".to_owned());
        }

        if !matches!(self.stop_bits, 1 | 2) {
            return Err("serial.stop_bits must be 1 or 2".to_owned());
        }

        if self.timeout_ms == 0 {
            return Err("serial.timeout_ms must be greater than zero".to_owned());
        }

        Ok(())
    }
}

impl Default for SerialPortSettings {
    fn default() -> Self {
        Self {
            main_port: String::new(),
            emulator_port: String::new(),
            baud_rate: DEFAULT_BAUD_RATE,
            data_bits: DEFAULT_DATA_BITS,
            parity: SerialParity::None,
            stop_bits: DEFAULT_STOP_BITS,
            flow_control: SerialFlowControl::None,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SerialParity {
    #[default]
    None,
    Even,
    Odd,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SerialFlowControl {
    #[default]
    None,
    Software,
    Hardware,
}

fn validate_port_name(field: &str, port: &str) -> Result<(), String> {
    if port.trim() != port {
        return Err(format!("{field} cannot begin or end with whitespace",));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{AppConfig, SerialFlowControl, SerialParity};

    use crate::{application_definition::ApplicationDefinition, connection::ConnectionId};

    #[test]
    fn uses_expected_defaults() {
        let config = AppConfig::default();

        assert_eq!(config.application.fps, 30);
        assert_eq!(config.application.poll_interval_ms, 1_000,);
        assert_eq!(config.application.max_plot_points_per_series, 4_000,);

        assert_eq!(config.serial.baud_rate, 9_600);
        assert_eq!(config.serial.data_bits, 8);
        assert_eq!(config.serial.parity, SerialParity::None,);
        assert_eq!(config.serial.flow_control, SerialFlowControl::None,);
        assert_eq!(config.application.plot_window_seconds, 3_600,);
        assert!(config.emulator.script_path.is_empty());
    }

    #[test]
    fn fills_missing_fields_with_defaults() {
        let config = AppConfig::parse(
            "\
[application]
fps = 60
",
        )
        .unwrap();

        assert_eq!(config.application.fps, 60);
        assert_eq!(config.application.poll_interval_ms, 1_000,);
        assert_eq!(config.application.max_plot_points_per_series, 4_000,);
        assert_eq!(config.serial.baud_rate, 9_600);
        assert_eq!(config.application.plot_window_seconds, 3_600,);
    }

    #[test]
    fn parses_serial_settings() {
        let config = AppConfig::parse(
            "\
[serial]
main_port = \"COM3\"
emulator_port = \"COM4\"
baud_rate = 115200
data_bits = 7
parity = \"even\"
stop_bits = 2
flow_control = \"hardware\"
timeout_ms = 500
",
        )
        .unwrap();

        assert_eq!(config.serial.main_port, "COM3");
        assert_eq!(config.serial.emulator_port, "COM4");
        assert_eq!(config.serial.baud_rate, 115_200);
        assert_eq!(config.serial.parity, SerialParity::Even,);
        assert_eq!(config.serial.flow_control, SerialFlowControl::Hardware,);
    }

    #[test]
    fn rejects_zero_poll_interval() {
        let result = AppConfig::parse(
            "\
[application]
poll_interval_ms = 0
",
        );

        assert_eq!(
            result.unwrap_err(),
            "application.poll_interval_ms must be \
             greater than zero",
        );
    }

    #[test]
    fn rejects_too_few_plot_points() {
        let result = AppConfig::parse(
            "\
[application]
max_plot_points_per_series = 3
",
        );

        assert_eq!(
            result.unwrap_err(),
            "application.max_plot_points_per_series \
             must be between 4 and 100000",
        );
    }

    #[test]
    fn rejects_same_serial_ports() {
        let result = AppConfig::parse(
            "\
[serial]
main_port = \"COM3\"
emulator_port = \"com3\"
",
        );

        assert_eq!(
            result.unwrap_err(),
            "serial.main_port and serial.emulator_port \
             must be different",
        );
    }

    #[test]
    fn rejects_whitespace_around_port() {
        let result = AppConfig::parse(
            "\
[serial]
main_port = \" COM3\"
",
        );

        assert_eq!(
            result.unwrap_err(),
            "serial.main_port cannot begin or end \
             with whitespace",
        );
    }

    #[test]
    fn missing_file_uses_defaults() {
        let path = std::env::temp_dir().join(format!(
            "missing_com_port_reader_config_{}.toml",
            std::process::id(),
        ));

        let (config, warning) = AppConfig::load_or_default(path);

        assert_eq!(config, AppConfig::default());
        assert_eq!(warning, None);
    }

    #[test]
    fn parses_emulator_settings() {
        let config = AppConfig::parse(
            "\
    [emulator]
    script_path = \"emulator_scripts/sine.lua\"
    ",
        )
        .unwrap();

        assert_eq!(config.emulator.script_path, "emulator_scripts/sine.lua",);
    }

    #[test]
    fn rejects_zero_plot_window() {
        let result = AppConfig::parse(
            "\
    [application]
    plot_window_seconds = 0
    ",
        );

        assert_eq!(
            result.unwrap_err(),
            "application.plot_window_seconds must be between \
             1 and 1209600",
        );
    }

    #[test]
    fn rejects_plot_window_longer_than_two_weeks() {
        let result = AppConfig::parse(
            "\
    [application]
    plot_window_seconds = 1209601
    ",
        );

        assert_eq!(
            result.unwrap_err(),
            "application.plot_window_seconds must be between \
             1 and 1209600",
        );
    }

    #[test]
    fn converts_config_to_application_definition() {
        let mut config = AppConfig::default();

        config.application.fps = 60;
        config.application.poll_interval_ms = 2_500;
        config.application.plot_window_seconds = 7_200;

        config.application.max_plot_points_per_series = 8_000;

        config.serial.main_port = "COM3".to_owned();
        config.serial.baud_rate = 115_200;
        config.serial.data_bits = 7;
        config.serial.parity = SerialParity::Even;
        config.serial.stop_bits = 2;

        config.serial.flow_control = SerialFlowControl::Hardware;

        config.serial.timeout_ms = 500;

        let definition = ApplicationDefinition::try_from(&config).unwrap();

        let runtime = definition.runtime();

        assert_eq!(runtime.fps(), 60);

        assert_eq!(
            runtime.default_poll_interval(),
            Duration::from_millis(2_500),
        );

        assert_eq!(runtime.plot_window(), Duration::from_secs(7_200),);

        assert_eq!(runtime.max_plot_points_per_series(), 8_000,);

        let connections = definition.serial_connections();

        assert_eq!(connections.len(), 1);

        let connection = &connections[0];

        assert_eq!(connection.id(), ConnectionId::PRIMARY,);

        assert_eq!(connection.name(), "primary");

        let serial = connection.serial_config();

        assert_eq!(serial.port_name(), "COM3");
        assert_eq!(serial.baud_rate(), 115_200);

        assert_eq!(serial.data_bits(), serialport::DataBits::Seven,);

        assert_eq!(serial.parity(), serialport::Parity::Even,);

        assert_eq!(serial.stop_bits(), serialport::StopBits::Two,);

        assert_eq!(serial.flow_control(), serialport::FlowControl::Hardware,);

        assert_eq!(serial.timeout_ms(), 500);
    }

    #[test]
    fn omits_unselected_serial_connection() {
        let config = AppConfig::default();

        let definition = ApplicationDefinition::try_from(&config).unwrap();

        assert!(definition.serial_connections().is_empty(),);
    }

    #[test]
    fn rejects_invalid_config_during_conversion() {
        let mut config = AppConfig::default();

        config.application.fps = 0;

        let error = ApplicationDefinition::try_from(&config).unwrap_err();

        assert_eq!(
            error.to_string(),
            "application.fps must be between 1 and 240",
        );
    }
}
