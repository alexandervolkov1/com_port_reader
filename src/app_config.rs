use std::{fs, path::Path, time::Duration};

use serde::{Deserialize, Serialize};

pub const CONFIG_PATH: &str = "config.toml";

const DEFAULT_FPS: u32 = 30;
const DEFAULT_POLL_INTERVAL_MS: u64 = 1_000;
const DEFAULT_BAUD_RATE: u32 = 9_600;
const DEFAULT_DATA_BITS: u8 = 8;
const DEFAULT_STOP_BITS: u8 = 1;
const DEFAULT_TIMEOUT_MS: u64 = 250;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub application: ApplicationSettings,
    pub main_serial: SerialPortSettings,
    pub emulator_serial: SerialPortSettings,
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

        validate_serial("main_serial", &self.main_serial)?;

        validate_serial("emulator_serial", &self.emulator_serial)?;

        let main_port = self.main_serial.port.trim();
        let emulator_port = self.emulator_serial.port.trim();

        if !main_port.is_empty() && main_port.eq_ignore_ascii_case(emulator_port) {
            return Err("main_serial.port and \
                 emulator_serial.port must be different"
                .to_owned());
        }

        Ok(())
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            application: ApplicationSettings::default(),

            main_serial: SerialPortSettings::default(),

            emulator_serial: SerialPortSettings::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ApplicationSettings {
    pub fps: u32,
    pub poll_interval_ms: u64,
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
            return Err("application.fps must be between \
                 1 and 240"
                .to_owned());
        }

        if self.poll_interval_ms == 0 {
            return Err("application.poll_interval_ms must \
                 be greater than zero"
                .to_owned());
        }

        Ok(())
    }
}

impl Default for ApplicationSettings {
    fn default() -> Self {
        Self {
            fps: DEFAULT_FPS,
            poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SerialPortSettings {
    pub port: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub parity: SerialParity,
    pub stop_bits: u8,
    pub flow_control: SerialFlowControl,
    pub timeout_ms: u64,
}

impl Default for SerialPortSettings {
    fn default() -> Self {
        Self {
            port: String::new(),
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

fn validate_serial(section: &str, settings: &SerialPortSettings) -> Result<(), String> {
    if settings.port.trim() != settings.port {
        return Err(format!(
            "{section}.port cannot begin or end \
             with whitespace",
        ));
    }

    if settings.baud_rate == 0 {
        return Err(format!(
            "{section}.baud_rate must be greater \
             than zero",
        ));
    }

    if !matches!(settings.data_bits, 5 | 6 | 7 | 8) {
        return Err(format!(
            "{section}.data_bits must be \
             5, 6, 7 or 8",
        ));
    }

    if !matches!(settings.stop_bits, 1 | 2) {
        return Err(format!("{section}.stop_bits must be 1 or 2",));
    }

    if settings.timeout_ms == 0 {
        return Err(format!(
            "{section}.timeout_ms must be greater \
             than zero",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AppConfig, SerialFlowControl, SerialParity};

    #[test]
    fn uses_expected_defaults() {
        let config = AppConfig::default();

        assert_eq!(config.application.fps, 30);

        assert_eq!(config.application.poll_interval_ms, 1_000,);

        assert_eq!(config.main_serial.baud_rate, 9_600,);

        assert_eq!(config.main_serial.data_bits, 8,);

        assert_eq!(config.main_serial.parity, SerialParity::None,);

        assert_eq!(config.main_serial.flow_control, SerialFlowControl::None,);
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

        assert_eq!(config.main_serial.baud_rate, 9_600,);
    }

    #[test]
    fn parses_serial_settings() {
        let config = AppConfig::parse(
            "\
[main_serial]
port = \"COM3\"
baud_rate = 115200
data_bits = 7
parity = \"even\"
stop_bits = 2
flow_control = \"hardware\"
timeout_ms = 500
",
        )
        .unwrap();

        assert_eq!(config.main_serial.port, "COM3",);

        assert_eq!(config.main_serial.baud_rate, 115_200,);

        assert_eq!(config.main_serial.parity, SerialParity::Even,);

        assert_eq!(config.main_serial.flow_control, SerialFlowControl::Hardware,);
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
            "application.poll_interval_ms must \
             be greater than zero",
        );
    }

    #[test]
    fn rejects_same_serial_ports() {
        let result = AppConfig::parse(
            "\
[main_serial]
port = \"COM3\"

[emulator_serial]
port = \"com3\"
",
        );

        assert_eq!(
            result.unwrap_err(),
            "main_serial.port and \
             emulator_serial.port must be different",
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
}
