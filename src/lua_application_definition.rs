use serialport::{DataBits, FlowControl, Parity, StopBits};
use std::{error::Error, fmt, fs, path::Path, time::Duration};

use mlua::{Lua, Table, Value};

use crate::{
    application_definition::{
        ApplicationDefinition, ApplicationScriptDefinition, EmulatorDefinition, RuntimeDefinition,
        SerialConnectionDefinition,
    },
    connection::ConnectionId,
    serial_connection::SerialPortConfig,
};

pub struct LoadedLuaDefinition {
    definition: ApplicationDefinition,
    source: Option<String>,
    warning: Option<String>,
}

impl LoadedLuaDefinition {
    fn new(
        definition: ApplicationDefinition,
        source: Option<String>,
        warning: Option<String>,
    ) -> Self {
        Self {
            definition,
            source,
            warning,
        }
    }

    pub fn into_parts(self) -> (ApplicationDefinition, Option<String>, Option<String>) {
        (self.definition, self.source, self.warning)
    }
}

pub fn load_lua_definition_or_base(
    path: impl AsRef<Path>,
    base: &ApplicationDefinition,
) -> LoadedLuaDefinition {
    let path = path.as_ref();

    let source = match fs::read_to_string(path) {
        Ok(source) => source,

        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return LoadedLuaDefinition::new(base.clone(), None, None);
        }

        Err(error) => {
            return LoadedLuaDefinition::new(
                base.clone(),
                None,
                Some(format!(
                    "Failed to read Lua \
                         application definition \
                         '{}': {error}. Internal defaults will be used.",
                    path.display(),
                )),
            );
        }
    };

    match apply_lua_definition(&source, base) {
        Ok(definition) => LoadedLuaDefinition::new(definition, Some(source), None),

        Err(error) => LoadedLuaDefinition::new(
            base.clone(),
            None,
            Some(format!(
                "Failed to load Lua \
                     application definition '{}': \
                     {error}. Internal defaults will be used.",
                path.display(),
            )),
        ),
    }
}

pub fn apply_lua_definition(
    source: &str,
    base: &ApplicationDefinition,
) -> Result<ApplicationDefinition, LuaApplicationDefinitionError> {
    let lua = Lua::new();

    let root = lua.load(source).eval::<Table>()?;
    validate_root_keys(&root)?;
    validate_setup_function(&root)?;

    let mut definition = base.clone();

    if let Some(application) = root.get::<Option<Table>>("application")? {
        let runtime = parse_runtime_definition(&application, definition.runtime())?;

        definition.set_runtime(runtime);
    }

    if let Some(connections) = root.get::<Option<Table>>("connections")? {
        let serial_connections = parse_serial_connections(&connections)?;

        definition
            .replace_serial_connections(serial_connections)
            .map_err(|error| LuaApplicationDefinitionError::new(error.to_string()))?;
    }

    if let Some(emulator) = root.get::<Option<Table>>("emulator")? {
        let emulator_definition = parse_emulator_definition(&emulator, &definition)?;

        definition
            .set_emulator(Some(emulator_definition))
            .map_err(|error| LuaApplicationDefinitionError::new(error.to_string()))?;
    }

    if let Some(scripts) = root.get::<Option<Table>>("scripts")? {
        let scripts = parse_application_scripts(&scripts)?;

        definition
            .replace_scripts(scripts)
            .map_err(|error| LuaApplicationDefinitionError::new(error.to_string()))?;
    }

    Ok(definition)
}

fn validate_setup_function(root: &Table) -> Result<(), LuaApplicationDefinitionError> {
    match root
        .get::<Value>("setup")
        .map_err(LuaApplicationDefinitionError::from)?
    {
        Value::Nil | Value::Function(_) => Ok(()),

        _ => Err(LuaApplicationDefinitionError::new(
            "Application definition section \
                 'setup' must be a function",
        )),
    }
}

fn validate_root_keys(root: &Table) -> Result<(), LuaApplicationDefinitionError> {
    for pair in root.pairs::<String, Value>() {
        let (key, _) = pair.map_err(LuaApplicationDefinitionError::from)?;

        if !matches!(
            key.as_str(),
            "application" | "connections" | "emulator" | "scripts" | "setup"
        ) {
            return Err(LuaApplicationDefinitionError::new(format!(
                "Unknown application definition section '{key}'",
            )));
        }
    }

    Ok(())
}

fn parse_application_scripts(
    scripts: &Table,
) -> Result<Vec<ApplicationScriptDefinition>, LuaApplicationDefinitionError> {
    let length = scripts.raw_len();

    for pair in scripts.pairs::<Value, Value>() {
        let (key, _) = pair.map_err(LuaApplicationDefinitionError::from)?;

        let Value::Integer(index) = key else {
            return Err(LuaApplicationDefinitionError::new(
                "Application definition section \
                 'scripts' must be an array",
            ));
        };

        let valid_index = usize::try_from(index)
            .ok()
            .is_some_and(|index| index >= 1 && index <= length);

        if !valid_index {
            return Err(LuaApplicationDefinitionError::new(
                "Application definition section \
                 'scripts' must be a continuous array",
            ));
        }
    }

    let mut definitions = Vec::with_capacity(length);

    for index in 1..=length {
        let path = scripts
            .raw_get::<Option<String>>(index)
            .map_err(LuaApplicationDefinitionError::from)?
            .ok_or_else(|| {
                LuaApplicationDefinitionError::new(format!(
                    "Application script #{index} \
                         must be a path string",
                ))
            })?;

        let definition = ApplicationScriptDefinition::new(path)
            .map_err(|error| LuaApplicationDefinitionError::new(error.to_string()))?;

        definitions.push(definition);
    }

    Ok(definitions)
}

fn parse_runtime_definition(
    application: &Table,
    base: &RuntimeDefinition,
) -> Result<RuntimeDefinition, LuaApplicationDefinitionError> {
    validate_application_keys(application)?;

    let fps = application
        .get::<Option<u32>>("fps")
        .map_err(LuaApplicationDefinitionError::from)?
        .unwrap_or(base.fps());

    let default_poll_interval =
        duration_option(application, "poll_interval", base.default_poll_interval())?;

    let plot_window = duration_option(application, "plot_window", base.plot_window())?;

    let max_plot_points_per_series = application
        .get::<Option<usize>>("max_plot_points_per_series")
        .map_err(LuaApplicationDefinitionError::from)?
        .unwrap_or(base.max_plot_points_per_series());

    RuntimeDefinition::new(
        fps,
        default_poll_interval,
        plot_window,
        max_plot_points_per_series,
    )
    .map_err(|error| LuaApplicationDefinitionError::new(error.to_string()))
}

fn parse_emulator_definition(
    emulator: &Table,
    application_definition: &ApplicationDefinition,
) -> Result<EmulatorDefinition, LuaApplicationDefinitionError> {
    validate_emulator_keys(emulator)?;

    let connection_name = emulator
        .get::<Option<String>>("connection")
        .map_err(LuaApplicationDefinitionError::from)?
        .ok_or_else(|| {
            LuaApplicationDefinitionError::new(
                "Lua emulator definition must \
                 contain 'connection'",
            )
        })?;

    let connection_id = application_definition
        .connection_id_by_name(&connection_name)
        .ok_or_else(|| {
            LuaApplicationDefinitionError::new(format!(
                "Unknown emulator connection \
                         '{connection_name}'",
            ))
        })?;

    let port_name = emulator
        .get::<Option<String>>("port")
        .map_err(LuaApplicationDefinitionError::from)?
        .ok_or_else(|| {
            LuaApplicationDefinitionError::new(
                "Lua emulator definition must \
                 contain 'port'",
            )
        })?;

    let script_path = emulator
        .get::<Option<String>>("script")
        .map_err(LuaApplicationDefinitionError::from)?
        .ok_or_else(|| {
            LuaApplicationDefinitionError::new(
                "Lua emulator definition must \
                 contain 'script'",
            )
        })?;

    EmulatorDefinition::new(connection_id, port_name, script_path)
        .map_err(|error| LuaApplicationDefinitionError::new(error.to_string()))
}

fn validate_emulator_keys(emulator: &Table) -> Result<(), LuaApplicationDefinitionError> {
    for pair in emulator.pairs::<String, Value>() {
        let (key, _) = pair.map_err(LuaApplicationDefinitionError::from)?;

        if !matches!(key.as_str(), "connection" | "port" | "script") {
            return Err(LuaApplicationDefinitionError::new(format!(
                "Unknown emulator option \
                         '{key}'",
            )));
        }
    }

    Ok(())
}

fn parse_serial_connections(
    connections: &Table,
) -> Result<Vec<SerialConnectionDefinition>, LuaApplicationDefinitionError> {
    let mut entries = Vec::new();

    for pair in connections.pairs::<String, Table>() {
        let (name, options) = pair.map_err(LuaApplicationDefinitionError::from)?;

        entries.push((name, options));
    }

    if !entries.is_empty()
        && !entries
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("primary"))
    {
        return Err(LuaApplicationDefinitionError::new(
            "Lua connections must contain a \
                 'primary' connection",
        ));
    }

    entries.sort_by(|(left, _), (right, _)| {
        let left_is_primary = left.eq_ignore_ascii_case("primary");

        let right_is_primary = right.eq_ignore_ascii_case("primary");

        right_is_primary
            .cmp(&left_is_primary)
            .then_with(|| left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase()))
    });

    let mut next_connection_id = 2_u64;

    let mut result = Vec::with_capacity(entries.len());

    for (name, options) in entries {
        let connection_id = if name.eq_ignore_ascii_case("primary") {
            ConnectionId::PRIMARY
        } else {
            let id = ConnectionId::new(next_connection_id);

            next_connection_id += 1;

            id
        };

        result.push(parse_serial_connection(connection_id, name, &options)?);
    }

    Ok(result)
}

fn parse_serial_connection(
    connection_id: ConnectionId,
    name: String,
    options: &Table,
) -> Result<SerialConnectionDefinition, LuaApplicationDefinitionError> {
    validate_serial_connection_keys(options)?;

    let port_name = options
        .get::<Option<String>>("port")
        .map_err(LuaApplicationDefinitionError::from)?
        .ok_or_else(|| {
            LuaApplicationDefinitionError::new(format!(
                "Connection '{name}' must define \
                     a COM port",
            ))
        })?;

    let baud_rate = options
        .get::<Option<u32>>("baud_rate")
        .map_err(LuaApplicationDefinitionError::from)?
        .unwrap_or(9_600);

    let data_bits = match options
        .get::<Option<u8>>("data_bits")
        .map_err(LuaApplicationDefinitionError::from)?
        .unwrap_or(8)
    {
        5 => DataBits::Five,
        6 => DataBits::Six,
        7 => DataBits::Seven,
        8 => DataBits::Eight,

        value => {
            return Err(LuaApplicationDefinitionError::new(format!(
                "Connection '{name}' data_bits \
                         must be 5, 6, 7 or 8, got \
                         {value}",
            )));
        }
    };

    let parity_name = options
        .get::<Option<String>>("parity")
        .map_err(LuaApplicationDefinitionError::from)?
        .unwrap_or_else(|| "none".to_owned());

    let parity = parse_parity(&name, &parity_name)?;

    let stop_bits = match options
        .get::<Option<u8>>("stop_bits")
        .map_err(LuaApplicationDefinitionError::from)?
        .unwrap_or(1)
    {
        1 => StopBits::One,
        2 => StopBits::Two,

        value => {
            return Err(LuaApplicationDefinitionError::new(format!(
                "Connection '{name}' stop_bits \
                         must be 1 or 2, got {value}",
            )));
        }
    };

    let flow_control_name = options
        .get::<Option<String>>("flow_control")
        .map_err(LuaApplicationDefinitionError::from)?
        .unwrap_or_else(|| "none".to_owned());

    let flow_control = parse_flow_control(&name, &flow_control_name)?;

    let timeout_seconds = options
        .get::<Option<f64>>("timeout")
        .map_err(LuaApplicationDefinitionError::from)?
        .unwrap_or(0.25);

    let timeout_ms = duration_milliseconds(&name, "timeout", timeout_seconds)?;

    let serial_config = SerialPortConfig::new(
        port_name,
        baud_rate,
        data_bits,
        parity,
        stop_bits,
        flow_control,
        timeout_ms,
    );

    SerialConnectionDefinition::new(connection_id, name, serial_config)
        .map_err(|error| LuaApplicationDefinitionError::new(error.to_string()))
}

fn validate_serial_connection_keys(options: &Table) -> Result<(), LuaApplicationDefinitionError> {
    for pair in options.pairs::<String, Value>() {
        let (key, _) = pair.map_err(LuaApplicationDefinitionError::from)?;

        if !matches!(
            key.as_str(),
            "port"
                | "baud_rate"
                | "data_bits"
                | "parity"
                | "stop_bits"
                | "flow_control"
                | "timeout"
        ) {
            return Err(LuaApplicationDefinitionError::new(format!(
                "Unknown serial connection \
                         option '{key}'",
            )));
        }
    }

    Ok(())
}

fn parse_parity(
    connection_name: &str,
    value: &str,
) -> Result<Parity, LuaApplicationDefinitionError> {
    match value.to_ascii_lowercase().as_str() {
        "none" => Ok(Parity::None),
        "even" => Ok(Parity::Even),
        "odd" => Ok(Parity::Odd),

        _ => Err(LuaApplicationDefinitionError::new(format!(
            "Connection '{connection_name}' \
                     parity must be 'none', 'even' or \
                     'odd', got '{value}'",
        ))),
    }
}

fn parse_flow_control(
    connection_name: &str,
    value: &str,
) -> Result<FlowControl, LuaApplicationDefinitionError> {
    match value.to_ascii_lowercase().as_str() {
        "none" => Ok(FlowControl::None),

        "software" => Ok(FlowControl::Software),

        "hardware" => Ok(FlowControl::Hardware),

        _ => Err(LuaApplicationDefinitionError::new(format!(
            "Connection '{connection_name}' \
                     flow_control must be 'none', \
                     'software' or 'hardware', got \
                     '{value}'",
        ))),
    }
}

fn duration_milliseconds(
    connection_name: &str,
    option_name: &str,
    seconds: f64,
) -> Result<u64, LuaApplicationDefinitionError> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err(LuaApplicationDefinitionError::new(format!(
            "Connection '{connection_name}' \
                     option '{option_name}' must be \
                     finite and greater than zero",
        )));
    }

    let duration = Duration::try_from_secs_f64(seconds).map_err(|_| {
        LuaApplicationDefinitionError::new(format!(
            "Connection \
                         '{connection_name}' option \
                         '{option_name}' is outside \
                         the supported duration range",
        ))
    })?;

    let milliseconds = u64::try_from(duration.as_millis()).map_err(|_| {
        LuaApplicationDefinitionError::new(format!(
            "Connection \
                         '{connection_name}' option \
                         '{option_name}' is too large",
        ))
    })?;

    if milliseconds == 0 {
        return Err(LuaApplicationDefinitionError::new(format!(
            "Connection '{connection_name}' \
                     option '{option_name}' must be \
                     at least 0.001 seconds",
        )));
    }

    Ok(milliseconds)
}

fn validate_application_keys(application: &Table) -> Result<(), LuaApplicationDefinitionError> {
    for pair in application.pairs::<String, Value>() {
        let (key, _) = pair.map_err(LuaApplicationDefinitionError::from)?;

        if !matches!(
            key.as_str(),
            "fps" | "poll_interval" | "plot_window" | "max_plot_points_per_series"
        ) {
            return Err(LuaApplicationDefinitionError::new(format!(
                "Unknown application option \
                         '{key}'",
            )));
        }
    }

    Ok(())
}

fn duration_option(
    table: &Table,
    key: &str,
    default: Duration,
) -> Result<Duration, LuaApplicationDefinitionError> {
    let Some(seconds) = table
        .get::<Option<f64>>(key)
        .map_err(LuaApplicationDefinitionError::from)?
    else {
        return Ok(default);
    };

    if !seconds.is_finite() || seconds <= 0.0 {
        return Err(LuaApplicationDefinitionError::new(format!(
            "Application option '{key}' must \
                     be finite and greater than zero",
        )));
    }

    Duration::try_from_secs_f64(seconds).map_err(|_| {
        LuaApplicationDefinitionError::new(format!(
            "Application option '{key}' is \
                     outside the supported duration \
                     range",
        ))
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LuaApplicationDefinitionError {
    message: String,
}

impl LuaApplicationDefinitionError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for LuaApplicationDefinitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for LuaApplicationDefinitionError {}

impl From<mlua::Error> for LuaApplicationDefinitionError {
    fn from(error: mlua::Error) -> Self {
        Self::new(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::Duration,
    };

    use serialport::{DataBits, FlowControl, Parity, StopBits};

    use super::{apply_lua_definition, load_lua_definition_or_base};

    use crate::{
        application_definition::{ApplicationDefinition, SerialConnectionDefinition},
        connection::ConnectionId,
        serial_connection::SerialPortConfig,
    };

    fn base_definition() -> ApplicationDefinition {
        let mut definition = ApplicationDefinition::default();

        let serial_config = SerialPortConfig::new(
            "COM3".to_owned(),
            9_600,
            DataBits::Eight,
            Parity::None,
            StopBits::One,
            FlowControl::None,
            250,
        );

        let connection =
            SerialConnectionDefinition::new(ConnectionId::PRIMARY, "primary", serial_config)
                .unwrap();

        definition.add_serial_connection(connection).unwrap();

        definition
    }

    fn temporary_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("com_port_reader_{name}_{}.lua", std::process::id(),))
    }

    #[test]
    fn applies_lua_runtime_settings() {
        let base = base_definition();

        let definition = apply_lua_definition(
            r#"
                return {
                    application = {
                        fps = 60,
                        poll_interval = 2.5,
                        plot_window = 7200.0,
                        max_plot_points_per_series =
                            8000,
                    },
                }
            "#,
            &base,
        )
        .unwrap();

        let runtime = definition.runtime();

        assert_eq!(runtime.fps(), 60);

        assert_eq!(
            runtime.default_poll_interval(),
            Duration::from_millis(2_500),
        );

        assert_eq!(runtime.plot_window(), Duration::from_secs(7_200),);

        assert_eq!(runtime.max_plot_points_per_series(), 8_000,);
    }

    #[test]
    fn preserves_existing_connections() {
        let base = base_definition();

        let definition = apply_lua_definition(
            r#"
                return {
                    application = {
                        fps = 40,
                    },
                }
            "#,
            &base,
        )
        .unwrap();

        assert_eq!(definition.serial_connections(), base.serial_connections(),);
    }

    #[test]
    fn preserves_missing_runtime_options() {
        let base = base_definition();

        let definition = apply_lua_definition(
            r#"
                return {
                    application = {
                        fps = 45,
                    },
                }
            "#,
            &base,
        )
        .unwrap();

        assert_eq!(definition.runtime().fps(), 45);

        assert_eq!(
            definition.runtime().default_poll_interval(),
            base.runtime().default_poll_interval(),
        );

        assert_eq!(
            definition.runtime().plot_window(),
            base.runtime().plot_window(),
        );
    }

    #[test]
    fn accepts_definition_without_application_section() {
        let base = base_definition();

        let definition = apply_lua_definition("return {}", &base).unwrap();

        assert_eq!(definition, base);
    }

    #[test]
    fn rejects_unknown_application_option() {
        let base = base_definition();

        let error = apply_lua_definition(
            r#"
                return {
                    application = {
                        poll_interwal = 1.0,
                    },
                }
            "#,
            &base,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Unknown application option \
             'poll_interwal'",
        );
    }

    #[test]
    fn rejects_invalid_duration() {
        let base = base_definition();

        let error = apply_lua_definition(
            r#"
                return {
                    application = {
                        poll_interval = 0,
                    },
                }
            "#,
            &base,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Application option 'poll_interval' must \
             be finite and greater than zero",
        );
    }

    #[test]
    fn requires_returned_table() {
        let base = base_definition();

        let error = apply_lua_definition("return 42", &base).unwrap_err();

        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn loads_definition_from_file() {
        let path = temporary_path("valid_definition");

        fs::write(
            &path,
            r#"
                return {
                    application = {
                        fps = 75,
                        poll_interval = 1.5,
                    },
                }
            "#,
        )
        .unwrap();

        let base = base_definition();

        let (definition, source, warning) = load_lua_definition_or_base(&path, &base).into_parts();

        let _ = fs::remove_file(&path);

        assert!(warning.is_none());
        assert!(source.is_some());

        assert_eq!(definition.runtime().fps(), 75,);

        assert_eq!(
            definition.runtime().default_poll_interval(),
            Duration::from_millis(1_500),
        );
    }

    #[test]
    fn uses_base_when_file_is_missing() {
        let path = temporary_path("missing_definition");

        let _ = fs::remove_file(&path);

        let base = base_definition();

        let (definition, source, warning) = load_lua_definition_or_base(&path, &base).into_parts();

        assert_eq!(definition, base);
        assert!(source.is_none());
        assert!(warning.is_none());
    }

    #[test]
    fn uses_base_when_file_is_invalid() {
        let path = temporary_path("invalid_definition");

        fs::write(&path, "return { application = { fps = 0 } }").unwrap();

        let base = base_definition();

        let (definition, source, warning) = load_lua_definition_or_base(&path, &base).into_parts();

        let _ = fs::remove_file(&path);

        assert_eq!(definition, base);
        assert!(source.is_none());

        assert!(
            warning
                .expect("invalid file must produce warning",)
                .contains(
                    "Failed to load Lua application \
                     definition",
                ),
        );
    }

    #[test]
    fn replaces_base_connections_from_lua() {
        let base = base_definition();

        let definition = apply_lua_definition(
            r#"
                return {
                    connections = {
                        primary = {
                            port = "COM7",
                            baud_rate = 115200,
                            data_bits = 7,
                            parity = "even",
                            stop_bits = 2,
                            flow_control = "hardware",
                            timeout = 0.5,
                        },

                        vacuum_bus = {
                            port = "COM8",
                        },
                    },
                }
            "#,
            &base,
        )
        .unwrap();

        let connections = definition.serial_connections();

        assert_eq!(connections.len(), 2);

        let primary = &connections[0];

        assert_eq!(primary.id(), ConnectionId::PRIMARY,);

        assert_eq!(primary.name(), "primary");

        let primary_serial = primary.serial_config();

        assert_eq!(primary_serial.port_name(), "COM7",);

        assert_eq!(primary_serial.baud_rate(), 115_200,);

        assert_eq!(primary_serial.data_bits(), DataBits::Seven,);

        assert_eq!(primary_serial.parity(), Parity::Even,);

        assert_eq!(primary_serial.stop_bits(), StopBits::Two,);

        assert_eq!(primary_serial.flow_control(), FlowControl::Hardware,);

        assert_eq!(primary_serial.timeout_ms(), 500,);

        let vacuum_bus = &connections[1];

        assert_eq!(vacuum_bus.id(), ConnectionId::new(2),);

        assert_eq!(vacuum_bus.name(), "vacuum_bus",);

        assert_eq!(vacuum_bus.serial_config().port_name(), "COM8",);

        assert_eq!(vacuum_bus.serial_config().baud_rate(), 9_600,);
    }

    #[test]
    fn rejects_connections_without_primary() {
        let base = base_definition();

        let error = apply_lua_definition(
            r#"
                return {
                    connections = {
                        secondary = {
                            port = "COM8",
                        },
                    },
                }
            "#,
            &base,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Lua connections must contain a 'primary' \
             connection",
        );
    }

    #[test]
    fn rejects_duplicate_connection_ports() {
        let base = base_definition();

        let error = apply_lua_definition(
            r#"
                return {
                    connections = {
                        primary = {
                            port = "COM7",
                        },

                        secondary = {
                            port = "com7",
                        },
                    },
                }
            "#,
            &base,
        )
        .unwrap_err();

        assert!(error.to_string().contains(
            "assigned to more than one \
                     connection",
        ),);
    }

    #[test]
    fn rejects_unknown_root_section() {
        let base = base_definition();

        let error = apply_lua_definition(
            r#"
                return {
                    application = {
                        fps = 30,
                    },

                    connetions = {
                        primary = {
                            port = "COM7",
                        },
                    },
                }
            "#,
            &base,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Unknown application definition section \
             'connetions'",
        );
    }

    #[test]
    fn parses_emulator_definition() {
        let base = base_definition();

        let definition = apply_lua_definition(
            r#"
                return {
                    emulator = {
                        connection = "primary",
                        port = "COM4",
                        script =
                            "emulator_scripts/device.lua",
                    },
                }
            "#,
            &base,
        )
        .unwrap();

        let emulator = definition
            .emulator()
            .expect("emulator definition must exist");

        assert_eq!(emulator.connection_id(), ConnectionId::PRIMARY,);

        assert_eq!(emulator.port_name(), "COM4",);

        assert_eq!(
            emulator.script_path(),
            std::path::Path::new("emulator_scripts/device.lua",),
        );
    }

    #[test]
    fn rejects_unknown_emulator_connection() {
        let base = base_definition();

        let error = apply_lua_definition(
            r#"
                return {
                    emulator = {
                        connection = "missing",
                        port = "COM4",
                        script = "device.lua",
                    },
                }
            "#,
            &base,
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "Unknown emulator connection 'missing'",);
    }

    #[test]
    fn accepts_setup_function() {
        let base = base_definition();

        let definition = apply_lua_definition(
            r#"
                return {
                    application = {
                        fps = 60,
                    },

                    setup = function()
                        return 42
                    end,
                }
            "#,
            &base,
        )
        .unwrap();

        assert_eq!(definition.runtime().fps(), 60,);
    }

    #[test]
    fn rejects_non_function_setup() {
        let base = base_definition();

        let error = apply_lua_definition(
            r#"
                return {
                    setup = "not a function",
                }
            "#,
            &base,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Application definition section 'setup' \
             must be a function",
        );
    }

    #[test]
    fn parses_application_scripts() {
        let base = base_definition();

        let definition = apply_lua_definition(
            r#"
                return {
                    scripts = {
                        "lua_scripts/sine_braid_demo.lua",
                        "lua_scripts/metakon_process.lua",
                    },
                }
            "#,
            &base,
        )
        .unwrap();

        let scripts = definition.scripts();

        assert_eq!(scripts.len(), 2);

        assert_eq!(
            scripts[0].path(),
            Path::new("lua_scripts/sine_braid_demo.lua",),
        );

        assert_eq!(
            scripts[1].path(),
            Path::new("lua_scripts/metakon_process.lua",),
        );
    }

    #[test]
    fn accepts_empty_application_script_list() {
        let base = base_definition();

        let definition = apply_lua_definition(
            r#"
                return {
                    scripts = {},
                }
            "#,
            &base,
        )
        .unwrap();

        assert!(definition.scripts().is_empty());
    }

    #[test]
    fn rejects_duplicate_application_scripts() {
        let base = base_definition();

        let error = apply_lua_definition(
            r#"
                return {
                    scripts = {
                        "lua_scripts/sine_braid_demo.lua",
                        "LUA_SCRIPTS/SINE_BRAID_DEMO.LUA",
                    },
                }
            "#,
            &base,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Application script \
             'LUA_SCRIPTS/SINE_BRAID_DEMO.LUA' \
             is defined more than once",
        );
    }

    #[test]
    fn rejects_non_array_script_definition() {
        let base = base_definition();

        let error = apply_lua_definition(
            r#"
                return {
                    scripts = {
                        demo =
                            "lua_scripts/sine_braid_demo.lua",
                    },
                }
            "#,
            &base,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Application definition section \
             'scripts' must be an array",
        );
    }
}
