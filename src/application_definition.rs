use std::{error::Error, fmt, time::Duration};

use crate::{connection::ConnectionId, serial_connection::SerialPortConfig};

const MIN_FPS: u32 = 1;
const MAX_FPS: u32 = 240;

const MIN_PLOT_WINDOW: Duration = Duration::from_secs(1);

const MAX_PLOT_WINDOW: Duration = Duration::from_secs(14 * 24 * 60 * 60);

const MIN_PLOT_POINTS_PER_SERIES: usize = 4;
const MAX_PLOT_POINTS_PER_SERIES: usize = 100_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationDefinition {
    runtime: RuntimeDefinition,
    serial_connections: Vec<SerialConnectionDefinition>,
}

impl ApplicationDefinition {
    pub fn new(runtime: RuntimeDefinition) -> Self {
        Self {
            runtime,
            serial_connections: Vec::new(),
        }
    }

    pub const fn runtime(&self) -> &RuntimeDefinition {
        &self.runtime
    }

    pub fn set_runtime(&mut self, runtime: RuntimeDefinition) {
        self.runtime = runtime;
    }

    pub fn serial_connections(&self) -> &[SerialConnectionDefinition] {
        &self.serial_connections
    }

    pub fn add_serial_connection(
        &mut self,
        connection: SerialConnectionDefinition,
    ) -> Result<(), ApplicationDefinitionError> {
        if self
            .serial_connections
            .iter()
            .any(|stored| stored.id() == connection.id())
        {
            return Err(ApplicationDefinitionError::new(format!(
                "Connection {} is defined more than once",
                connection.id(),
            )));
        }

        if self
            .serial_connections
            .iter()
            .any(|stored| stored.name().eq_ignore_ascii_case(connection.name()))
        {
            return Err(ApplicationDefinitionError::new(format!(
                "Connection name '{}' is defined \
                     more than once",
                connection.name(),
            )));
        }

        if self.serial_connections.iter().any(|stored| {
            stored
                .serial_config()
                .port_name()
                .eq_ignore_ascii_case(connection.serial_config().port_name())
        }) {
            return Err(ApplicationDefinitionError::new(format!(
                "COM port '{}' is assigned to more \
                     than one connection",
                connection.serial_config().port_name(),
            )));
        }

        self.serial_connections.push(connection);

        Ok(())
    }

    pub fn replace_serial_connections(
        &mut self,
        connections: impl IntoIterator<Item = SerialConnectionDefinition>,
    ) -> Result<(), ApplicationDefinitionError> {
        let mut validated = ApplicationDefinition::new(self.runtime.clone());

        for connection in connections {
            validated.add_serial_connection(connection)?;
        }

        self.serial_connections = validated.serial_connections;

        Ok(())
    }
}

impl Default for ApplicationDefinition {
    fn default() -> Self {
        Self::new(RuntimeDefinition::default())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeDefinition {
    fps: u32,
    default_poll_interval: Duration,
    plot_window: Duration,
    max_plot_points_per_series: usize,
}

impl RuntimeDefinition {
    pub fn new(
        fps: u32,
        default_poll_interval: Duration,
        plot_window: Duration,
        max_plot_points_per_series: usize,
    ) -> Result<Self, ApplicationDefinitionError> {
        if !(MIN_FPS..=MAX_FPS).contains(&fps) {
            return Err(ApplicationDefinitionError::new(format!(
                "FPS must be between {MIN_FPS} \
                     and {MAX_FPS}",
            )));
        }

        if default_poll_interval.is_zero() {
            return Err(ApplicationDefinitionError::new(
                "Default polling interval must be \
                 greater than zero",
            ));
        }

        if !(MIN_PLOT_WINDOW..=MAX_PLOT_WINDOW).contains(&plot_window) {
            return Err(ApplicationDefinitionError::new(format!(
                "Plot window must be between {} \
                     and {} seconds",
                MIN_PLOT_WINDOW.as_secs(),
                MAX_PLOT_WINDOW.as_secs(),
            )));
        }

        if !(MIN_PLOT_POINTS_PER_SERIES..=MAX_PLOT_POINTS_PER_SERIES)
            .contains(&max_plot_points_per_series)
        {
            return Err(ApplicationDefinitionError::new(format!(
                "Maximum plot points per series \
                     must be between \
                     {MIN_PLOT_POINTS_PER_SERIES} and \
                     {MAX_PLOT_POINTS_PER_SERIES}",
            )));
        }

        Ok(Self {
            fps,
            default_poll_interval,
            plot_window,
            max_plot_points_per_series,
        })
    }

    pub const fn fps(&self) -> u32 {
        self.fps
    }

    pub const fn default_poll_interval(&self) -> Duration {
        self.default_poll_interval
    }

    pub const fn plot_window(&self) -> Duration {
        self.plot_window
    }

    pub const fn max_plot_points_per_series(&self) -> usize {
        self.max_plot_points_per_series
    }

    pub fn repaint_interval(&self) -> Duration {
        Duration::from_secs_f64(1.0 / f64::from(self.fps))
    }
}

impl Default for RuntimeDefinition {
    fn default() -> Self {
        Self {
            fps: 30,
            default_poll_interval: Duration::from_secs(1),
            plot_window: Duration::from_secs(3_600),
            max_plot_points_per_series: 4_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerialConnectionDefinition {
    id: ConnectionId,
    name: String,
    serial_config: SerialPortConfig,
}

impl SerialConnectionDefinition {
    pub fn new(
        id: ConnectionId,
        name: impl Into<String>,
        serial_config: SerialPortConfig,
    ) -> Result<Self, ApplicationDefinitionError> {
        if id.value() == 0 {
            return Err(ApplicationDefinitionError::new(
                "Connection ID must be greater than zero",
            ));
        }

        let name = name.into();

        if name.is_empty() {
            return Err(ApplicationDefinitionError::new(
                "Connection name cannot be empty",
            ));
        }

        if name.trim() != name {
            return Err(ApplicationDefinitionError::new(
                "Connection name cannot begin or end \
                 with whitespace",
            ));
        }

        let port_name = serial_config.port_name();

        if port_name.is_empty() {
            return Err(ApplicationDefinitionError::new(
                "COM port name cannot be empty",
            ));
        }

        if port_name.trim() != port_name {
            return Err(ApplicationDefinitionError::new(
                "COM port name cannot begin or end \
                 with whitespace",
            ));
        }

        if serial_config.baud_rate() == 0 {
            return Err(ApplicationDefinitionError::new(
                "COM port baud rate must be greater \
                 than zero",
            ));
        }

        if serial_config.timeout_ms() == 0 {
            return Err(ApplicationDefinitionError::new(
                "COM port timeout must be greater \
                 than zero",
            ));
        }

        Ok(Self {
            id,
            name,
            serial_config,
        })
    }

    pub const fn id(&self) -> ConnectionId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn serial_config(&self) -> &SerialPortConfig {
        &self.serial_config
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationDefinitionError {
    message: String,
}

impl ApplicationDefinitionError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ApplicationDefinitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ApplicationDefinitionError {}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serialport::{DataBits, FlowControl, Parity, StopBits};

    use super::{ApplicationDefinition, RuntimeDefinition, SerialConnectionDefinition};

    use crate::{connection::ConnectionId, serial_connection::SerialPortConfig};

    fn serial_config(port_name: &str) -> SerialPortConfig {
        SerialPortConfig::new(
            port_name.to_owned(),
            9_600,
            DataBits::Eight,
            Parity::None,
            StopBits::One,
            FlowControl::None,
            250,
        )
    }

    fn connection(id: u64, name: &str, port_name: &str) -> SerialConnectionDefinition {
        SerialConnectionDefinition::new(ConnectionId::new(id), name, serial_config(port_name))
            .unwrap()
    }

    #[test]
    fn uses_runtime_defaults() {
        let runtime = RuntimeDefinition::default();

        assert_eq!(runtime.fps(), 30);

        assert_eq!(runtime.default_poll_interval(), Duration::from_secs(1),);

        assert_eq!(runtime.plot_window(), Duration::from_secs(3_600),);

        assert_eq!(runtime.max_plot_points_per_series(), 4_000,);
    }

    #[test]
    fn validates_runtime_definition() {
        assert!(
            RuntimeDefinition::new(0, Duration::from_secs(1), Duration::from_secs(3_600), 4_000,)
                .is_err(),
        );

        assert!(
            RuntimeDefinition::new(30, Duration::ZERO, Duration::from_secs(3_600), 4_000,).is_err(),
        );

        assert!(
            RuntimeDefinition::new(30, Duration::from_secs(1), Duration::ZERO, 4_000,).is_err(),
        );

        assert!(
            RuntimeDefinition::new(30, Duration::from_secs(1), Duration::from_secs(3_600), 3,)
                .is_err(),
        );
    }

    #[test]
    fn adds_serial_connections() {
        let mut definition = ApplicationDefinition::default();

        definition
            .add_serial_connection(connection(1, "primary", "COM3"))
            .unwrap();

        definition
            .add_serial_connection(connection(2, "emulator", "COM4"))
            .unwrap();

        assert_eq!(definition.serial_connections().len(), 2,);

        assert_eq!(
            definition.serial_connections()[0].id(),
            ConnectionId::PRIMARY,
        );
    }

    #[test]
    fn rejects_duplicate_connection_id() {
        let mut definition = ApplicationDefinition::default();

        definition
            .add_serial_connection(connection(1, "primary", "COM3"))
            .unwrap();

        let error = definition
            .add_serial_connection(connection(1, "another", "COM4"))
            .unwrap_err();

        assert_eq!(error.to_string(), "Connection 1 is defined more than once",);
    }

    #[test]
    fn rejects_duplicate_connection_name() {
        let mut definition = ApplicationDefinition::default();

        definition
            .add_serial_connection(connection(1, "primary", "COM3"))
            .unwrap();

        let error = definition
            .add_serial_connection(connection(2, "PRIMARY", "COM4"))
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Connection name 'PRIMARY' is defined \
             more than once",
        );
    }

    #[test]
    fn rejects_duplicate_serial_port() {
        let mut definition = ApplicationDefinition::default();

        definition
            .add_serial_connection(connection(1, "primary", "COM3"))
            .unwrap();

        let error = definition
            .add_serial_connection(connection(2, "secondary", "com3"))
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "COM port 'com3' is assigned to more \
             than one connection",
        );
    }

    #[test]
    fn rejects_invalid_connection_definition() {
        assert!(
            SerialConnectionDefinition::new(
                ConnectionId::new(0),
                "invalid",
                serial_config("COM3"),
            )
            .is_err(),
        );

        assert!(
            SerialConnectionDefinition::new(ConnectionId::PRIMARY, "", serial_config("COM3"),)
                .is_err(),
        );

        assert!(
            SerialConnectionDefinition::new(ConnectionId::PRIMARY, "primary", serial_config(""),)
                .is_err(),
        );
    }

    #[test]
    fn replaces_serial_connections() {
        let mut definition = ApplicationDefinition::default();

        definition
            .add_serial_connection(connection(1, "primary", "COM3"))
            .unwrap();

        definition
            .replace_serial_connections([
                connection(1, "primary", "COM7"),
                connection(2, "secondary", "COM8"),
            ])
            .unwrap();

        assert_eq!(definition.serial_connections().len(), 2,);

        assert_eq!(
            definition.serial_connections()[0]
                .serial_config()
                .port_name(),
            "COM7",
        );
    }
}
