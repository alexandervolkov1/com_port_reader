use std::{error::Error, fmt, fs, path::Path, time::Duration};

use mlua::{Lua, Table, Value};

use crate::application_definition::{ApplicationDefinition, RuntimeDefinition};

pub const STARTUP_SCRIPT_PATH: &str = "startup.lua";

pub fn load_lua_definition_or_base(
    path: impl AsRef<Path>,
    base: &ApplicationDefinition,
) -> (ApplicationDefinition, Option<String>) {
    let path = path.as_ref();

    let source = match fs::read_to_string(path) {
        Ok(source) => source,

        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (base.clone(), None);
        }

        Err(error) => {
            return (
                base.clone(),
                Some(format!(
                    "Failed to read Lua application \
                     definition '{}': {error}. \
                     TOML settings will be used.",
                    path.display(),
                )),
            );
        }
    };

    match apply_lua_definition(&source, base) {
        Ok(definition) => (definition, None),

        Err(error) => (
            base.clone(),
            Some(format!(
                "Failed to load Lua application \
                 definition '{}': {error}. \
                 TOML settings will be used.",
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

    let root = lua
        .load(source)
        .set_name("application definition")
        .eval::<Table>()
        .map_err(LuaApplicationDefinitionError::from)?;

    let Some(application) = root
        .get::<Option<Table>>("application")
        .map_err(LuaApplicationDefinitionError::from)?
    else {
        return Ok(base.clone());
    };

    validate_application_keys(&application)?;

    let base_runtime = base.runtime();

    let fps = application
        .get::<Option<u32>>("fps")
        .map_err(LuaApplicationDefinitionError::from)?
        .unwrap_or(base_runtime.fps());

    let default_poll_interval = duration_option(
        &application,
        "poll_interval",
        base_runtime.default_poll_interval(),
    )?;

    let plot_window = duration_option(&application, "plot_window", base_runtime.plot_window())?;

    let max_plot_points_per_series = application
        .get::<Option<usize>>("max_plot_points_per_series")
        .map_err(LuaApplicationDefinitionError::from)?
        .unwrap_or(base_runtime.max_plot_points_per_series());

    let runtime = RuntimeDefinition::new(
        fps,
        default_poll_interval,
        plot_window,
        max_plot_points_per_series,
    )
    .map_err(|error| LuaApplicationDefinitionError::new(error.to_string()))?;

    let mut definition = base.clone();

    definition.set_runtime(runtime);

    Ok(definition)
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
    use std::{fs, path::PathBuf, time::Duration};

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

        let (definition, warning) = load_lua_definition_or_base(&path, &base);

        let _ = fs::remove_file(&path);

        assert_eq!(warning, None);

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

        let (definition, warning) = load_lua_definition_or_base(&path, &base);

        assert_eq!(definition, base);
        assert_eq!(warning, None);
    }

    #[test]
    fn uses_base_when_file_is_invalid() {
        let path = temporary_path("invalid_definition");

        fs::write(&path, "return { application = { fps = 0 } }").unwrap();

        let base = base_definition();

        let (definition, warning) = load_lua_definition_or_base(&path, &base);

        let _ = fs::remove_file(&path);

        assert_eq!(definition, base);

        assert!(
            warning
                .expect("invalid file must produce warning")
                .contains(
                    "Failed to load Lua application \
                     definition",
                ),
        );
    }
}
