//! SQLite schema and process-record persistence for a single application session.
//!
//! Stores configuration snapshots, action lifecycles, logs, measurements and controller output results.
//! The owning recorder thread serializes writes; writer destruction marks session completion.

use std::{
    fmt::Display,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, params};

use super::{
    ProcessAction, ProcessActionId, ProcessActionOrigin, ProcessActionResult, ProcessControlOutput,
    ProcessLogLevel, ProcessMeasurement, ProcessRecord, ProcessRecordWriter, ProcessRecorderError,
};
use crate::data::SeriesId;

pub(crate) struct SqliteProcessRecordWriter {
    connection: Connection,
}

impl SqliteProcessRecordWriter {
    pub(crate) fn create(path: impl AsRef<Path>) -> Result<Self, ProcessRecorderError> {
        let path = path.as_ref();

        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|error| {
                recorder_error(
                    format!(
                        "Failed to create process database directory '{}'",
                        parent.display(),
                    ),
                    error,
                )
            })?;
        }

        let connection = Connection::open(path).map_err(|error| {
            recorder_error(
                format!("Failed to open process database '{}'", path.display()),
                error,
            )
        })?;

        connection
            .execute_batch(
                "
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = NORMAL;
                PRAGMA foreign_keys = ON;

                CREATE TABLE IF NOT EXISTS session (
                    id                  INTEGER PRIMARY KEY CHECK (id = 1),
                    started_at          REAL NOT NULL,
                    ended_at            REAL,
                    application_version TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS configurations (
                    id           INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp    REAL NOT NULL,
                    startup_path TEXT NOT NULL,
                    source       TEXT
                );

                CREATE TABLE IF NOT EXISTS logs (
                    id        INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp REAL NOT NULL,
                    level     TEXT NOT NULL,
                    message   TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS actions (
                    id            INTEGER PRIMARY KEY,
                    timestamp     REAL NOT NULL,
                    origin        TEXT NOT NULL,
                    action_type   TEXT NOT NULL,
                    connection_id TEXT,
                    series_id     TEXT,
                    series_name   TEXT,
                    details       TEXT NOT NULL,
                    status        TEXT NOT NULL,
                    completed_at  REAL,
                    result        TEXT,
                    error         TEXT
                );

                CREATE TABLE IF NOT EXISTS measurements (
                    id            INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp     REAL NOT NULL,
                    connection_id TEXT NOT NULL,
                    series_id     TEXT NOT NULL,
                    series_name   TEXT NOT NULL,
                    value         REAL NOT NULL
                );

                CREATE TABLE IF NOT EXISTS control_outputs (
                    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp            REAL NOT NULL,
                    loop_name            TEXT NOT NULL,
                    controller_kind      TEXT NOT NULL,
                    input_series_id      TEXT NOT NULL,
                    connection_id        TEXT NOT NULL,
                    setpoint             REAL,
                    measurement          REAL NOT NULL,
                    requested_output     REAL NOT NULL,
                    actual_output        REAL,
                    unconstrained_output REAL,
                    proportional         REAL,
                    integral             REAL,
                    derivative           REAL,
                    saturated            INTEGER
                );

                CREATE VIEW IF NOT EXISTS process_timeline AS

                SELECT
                    timestamp,
                    'log' AS event_type,
                    level,
                    NULL AS action_id,
                    NULL AS origin,
                    NULL AS action_type,
                    NULL AS connection_id,
                    NULL AS series_id,
                    NULL AS series_name,
                    message,
                    NULL AS details
                FROM logs

                UNION ALL

                SELECT
                    timestamp,
                    'configuration_loaded' AS event_type,
                    'info' AS level,
                    NULL AS action_id,
                    NULL AS origin,
                    NULL AS action_type,
                    NULL AS connection_id,
                    NULL AS series_id,
                    NULL AS series_name,
                    startup_path AS message,
                    source AS details
                FROM configurations

                UNION ALL

                SELECT
                    timestamp,
                    'action_requested' AS event_type,
                    'info' AS level,
                    id AS action_id,
                    origin,
                    action_type,
                    connection_id,
                    series_id,
                    series_name,
                    NULL AS message,
                    details
                FROM actions

                UNION ALL

                SELECT
                    completed_at AS timestamp,
                    'action_applied' AS event_type,
                    'info' AS level,
                    id AS action_id,
                    origin,
                    action_type,
                    connection_id,
                    series_id,
                    series_name,
                    NULL AS message,
                    result AS details
                FROM actions
                WHERE status = 'applied'
                  AND completed_at IS NOT NULL

                UNION ALL

                SELECT
                    completed_at AS timestamp,
                    'action_failed' AS event_type,
                    'error' AS level,
                    id AS action_id,
                    origin,
                    action_type,
                    connection_id,
                    series_id,
                    series_name,
                    error AS message,
                    NULL AS details
                FROM actions
                WHERE status = 'failed'
                  AND completed_at IS NOT NULL;

                CREATE INDEX IF NOT EXISTS measurements_timestamp_index
                    ON measurements(timestamp);

                CREATE INDEX IF NOT EXISTS measurements_series_index
                    ON measurements(series_id, timestamp);

                CREATE INDEX IF NOT EXISTS control_outputs_timestamp_index
                    ON control_outputs(timestamp);

                CREATE INDEX IF NOT EXISTS control_outputs_loop_index
                    ON control_outputs(loop_name, timestamp);

                CREATE INDEX IF NOT EXISTS logs_timestamp_index
                    ON logs(timestamp);

                CREATE INDEX IF NOT EXISTS actions_timestamp_index
                    ON actions(timestamp);
                ",
            )
            .map_err(|error| recorder_error("Failed to initialize process database", error))?;

        let started_at = system_time_seconds(SystemTime::now())?;

        connection
            .execute(
                "
                INSERT OR REPLACE INTO session (
                    id,
                    started_at,
                    ended_at,
                    application_version
                )
                VALUES (1, ?1, NULL, ?2)
                ",
                params![started_at, env!("CARGO_PKG_VERSION")],
            )
            .map_err(|error| {
                recorder_error("Failed to write process session information", error)
            })?;

        Ok(Self { connection })
    }

    fn write_configuration(
        &self,
        timestamp: SystemTime,
        startup_path: PathBuf,
        source: Option<String>,
    ) -> Result<(), ProcessRecorderError> {
        self.connection
            .execute(
                "
                INSERT INTO configurations (
                    timestamp,
                    startup_path,
                    source
                )
                VALUES (?1, ?2, ?3)
                ",
                params![
                    system_time_seconds(timestamp)?,
                    startup_path.to_string_lossy(),
                    source,
                ],
            )
            .map_err(|error| recorder_error("Failed to write configuration record", error))?;

        Ok(())
    }

    fn write_log(
        &self,
        timestamp: SystemTime,
        level: ProcessLogLevel,
        message: String,
    ) -> Result<(), ProcessRecorderError> {
        self.connection
            .execute(
                "
                INSERT INTO logs (
                    timestamp,
                    level,
                    message
                )
                VALUES (?1, ?2, ?3)
                ",
                params![
                    system_time_seconds(timestamp)?,
                    log_level_name(level),
                    message,
                ],
            )
            .map_err(|error| recorder_error("Failed to write log record", error))?;

        Ok(())
    }

    fn write_action(
        &self,
        action_id: ProcessActionId,
        timestamp: SystemTime,
        origin: ProcessActionOrigin,
        action: ProcessAction,
    ) -> Result<(), ProcessRecorderError> {
        let action_id = sqlite_action_id(action_id)?;

        let action_type = action_type_name(&action);
        let connection_id = action_connection_id(&action);
        let series_id = action_series_id(&action);
        let series_name = action_series_name(&action);
        let details = format!("{action:?}");

        self.connection
            .execute(
                "
                INSERT INTO actions (
                    id,
                    timestamp,
                    origin,
                    action_type,
                    connection_id,
                    series_id,
                    series_name,
                    details,
                    status
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ",
                params![
                    action_id,
                    system_time_seconds(timestamp)?,
                    action_origin_name(origin),
                    action_type,
                    connection_id,
                    series_id,
                    series_name,
                    details,
                    "requested",
                ],
            )
            .map_err(|error| recorder_error("Failed to write action record", error))?;

        Ok(())
    }

    fn write_action_applied(
        &self,
        action_id: ProcessActionId,
        timestamp: SystemTime,
        series_id: Option<SeriesId>,
        series_name: Option<String>,
        result: Option<ProcessActionResult>,
    ) -> Result<(), ProcessRecorderError> {
        let action_id = sqlite_action_id(action_id)?;
        let series_id = series_id.map(|series_id| series_id.to_string());
        let result = result.map(|result| format!("{result:?}"));

        self.connection
            .execute(
                "
                UPDATE actions
                SET
                    status = 'applied',
                    completed_at = ?2,
                    error = NULL,
                    series_id =
                        COALESCE(
                            ?3,
                            series_id
                        ),
                    series_name =
                        COALESCE(
                            ?4,
                            series_name
                        ),
                    result = ?5
                WHERE id = ?1
                ",
                params![
                    action_id,
                    system_time_seconds(timestamp)?,
                    series_id,
                    series_name,
                    result,
                ],
            )
            .map_err(|error| recorder_error("Failed to complete action record", error))?;

        Ok(())
    }

    fn write_action_failed(
        &self,
        action_id: ProcessActionId,
        timestamp: SystemTime,
        error: String,
    ) -> Result<(), ProcessRecorderError> {
        let action_id = sqlite_action_id(action_id)?;
        self.connection
            .execute(
                "
                UPDATE actions
                SET
                    status = 'failed',
                    completed_at = ?2,
                    error = ?3
                WHERE id = ?1
                ",
                params![action_id, system_time_seconds(timestamp)?, error,],
            )
            .map_err(|error| recorder_error("Failed to fail action record", error))?;

        Ok(())
    }

    fn write_measurements(
        &mut self,
        measurements: Vec<ProcessMeasurement>,
    ) -> Result<(), ProcessRecorderError> {
        if measurements.is_empty() {
            return Ok(());
        }

        let transaction = self
            .connection
            .transaction()
            .map_err(|error| recorder_error("Failed to start measurement transaction", error))?;

        {
            let mut statement = transaction
                .prepare(
                    "
                    INSERT INTO measurements (
                        timestamp,
                        connection_id,
                        series_id,
                        series_name,
                        value
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5)
                    ",
                )
                .map_err(|error| {
                    recorder_error("Failed to prepare measurement insertion", error)
                })?;

            for measurement in measurements {
                statement
                    .execute(params![
                        measurement.timestamp,
                        measurement.connection_id.value().to_string(),
                        measurement.series_id.to_string(),
                        measurement.series_name,
                        measurement.value,
                    ])
                    .map_err(|error| recorder_error("Failed to write measurement record", error))?;
            }
        }

        transaction
            .commit()
            .map_err(|error| recorder_error("Failed to commit measurements", error))?;

        Ok(())
    }

    fn write_control_output(
        &self,
        output: ProcessControlOutput,
    ) -> Result<(), ProcessRecorderError> {
        self.connection
            .execute(
                "
                INSERT INTO control_outputs (
                    timestamp,
                    loop_name,
                    controller_kind,
                    input_series_id,
                    connection_id,
                    setpoint,
                    measurement,
                    requested_output,
                    actual_output,
                    unconstrained_output,
                    proportional,
                    integral,
                    derivative,
                    saturated
                )
                VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                    ?8, ?9, ?10, ?11, ?12, ?13, ?14
                )
                ",
                params![
                    output.timestamp,
                    output.loop_name,
                    output.controller_kind,
                    output.input_series_id.to_string(),
                    output.connection_id.value().to_string(),
                    output.setpoint,
                    output.measurement,
                    output.requested_output,
                    output.actual_output,
                    output.unconstrained_output,
                    output.proportional,
                    output.integral,
                    output.derivative,
                    output.saturated,
                ],
            )
            .map_err(|error| recorder_error("Failed to write control output", error))?;

        Ok(())
    }
}

impl ProcessRecordWriter for SqliteProcessRecordWriter {
    fn write(&mut self, record: ProcessRecord) -> Result<(), ProcessRecorderError> {
        match record {
            ProcessRecord::ConfigurationLoaded {
                timestamp,
                startup_path,
                source,
            } => self.write_configuration(timestamp, startup_path, source),

            ProcessRecord::Log {
                timestamp,
                level,
                message,
            } => self.write_log(timestamp, level, message),

            ProcessRecord::ActionRequested {
                action_id,
                timestamp,
                origin,
                action,
            } => self.write_action(action_id, timestamp, origin, action),

            ProcessRecord::ActionApplied {
                action_id,
                timestamp,
                series_id,
                series_name,
                result,
            } => self.write_action_applied(action_id, timestamp, series_id, series_name, result),

            ProcessRecord::ActionFailed {
                action_id,
                timestamp,
                error,
            } => self.write_action_failed(action_id, timestamp, error),

            ProcessRecord::ControlOutput { output } => self.write_control_output(output),

            ProcessRecord::Measurements { measurements } => self.write_measurements(measurements),
        }
    }
}

impl Drop for SqliteProcessRecordWriter {
    fn drop(&mut self) {
        let Ok(ended_at) = system_time_seconds(SystemTime::now()) else {
            return;
        };

        let _ = self.connection.execute(
            "
            UPDATE session
            SET ended_at = ?1
            WHERE id = 1
            ",
            params![ended_at],
        );
    }
}

pub(crate) fn new_process_database_path(root_directory: impl AsRef<Path>) -> PathBuf {
    let now = chrono::Local::now();

    let directory = root_directory
        .as_ref()
        .join(now.format("%Y-%m-%d").to_string());

    let file_name = format!(
        "process_{}_pid-{}.sqlite3",
        now.format("%H-%M-%S%.3f"),
        std::process::id(),
    );

    directory.join(file_name)
}

fn system_time_seconds(timestamp: SystemTime) -> Result<f64, ProcessRecorderError> {
    timestamp
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .map_err(|error| recorder_error("Invalid process record timestamp", error))
}

fn log_level_name(level: ProcessLogLevel) -> &'static str {
    match level {
        ProcessLogLevel::Info => "info",
        ProcessLogLevel::Error => "error",
    }
}

fn action_origin_name(origin: ProcessActionOrigin) -> &'static str {
    match origin {
        ProcessActionOrigin::UserInterface => "user_interface",

        ProcessActionOrigin::Lua => "lua",
    }
}

fn action_type_name(action: &ProcessAction) -> &'static str {
    match action {
        ProcessAction::StartAcquisition => "start_acquisition",
        ProcessAction::StopAcquisition => "stop_acquisition",
        ProcessAction::ClearSeries => "clear_series",
        ProcessAction::StartEmulator => "start_emulator",
        ProcessAction::StopEmulator => "stop_emulator",
        ProcessAction::AddSeries { .. } => "add_series",
        ProcessAction::AddController { .. } => "add_controller",
        ProcessAction::WriteControllerParameter { .. } => "write_controller_parameter",
        ProcessAction::ConfigureController { .. } => "configure_controller",
        ProcessAction::WriteControllerReferenceParameter { .. } => {
            "write_controller_reference_parameter"
        }
        ProcessAction::ConfigureControllerReference { .. } => "configure_controller_reference",
        ProcessAction::SetControllerReference { .. } => "set_controller_reference",
        ProcessAction::SetControllerInput { .. } => "set_controller_input",
        ProcessAction::PauseController { .. } => "pause_controller",
        ProcessAction::RemoveController { .. } => "remove_controller",
        ProcessAction::ResumeController { .. } => "resume_controller",
        ProcessAction::ResetControllerIntegral { .. } => "reset_controller_integral",
        ProcessAction::ResetController { .. } => "reset_controller",
        ProcessAction::AddFilteredSeries { .. } => "add_filtered_series",
        ProcessAction::SetFilter { .. } => "set_filter",
        ProcessAction::DeleteSeriesByName { .. } => "delete_series_by_name",
        ProcessAction::RenameSeries { .. } => "rename_series",
        ProcessAction::SetSeriesVisibility { .. } => "set_series_visibility",
        ProcessAction::SendSerial { .. } => "send_serial",
        ProcessAction::ReadInstrument { .. } => "read_instrument",
        ProcessAction::WriteInstrument { .. } => "write_instrument",
        ProcessAction::DescribeVirtualInstruments { .. } => "describe_virtual_instruments",
        ProcessAction::SetSeriesColor { .. } => "set_series_color",
    }
}

fn action_connection_id(action: &ProcessAction) -> Option<String> {
    let connection_id = match action {
        ProcessAction::AddSeries { connection_id, .. }
        | ProcessAction::AddController { connection_id, .. }
        | ProcessAction::SendSerial { connection_id, .. }
        | ProcessAction::ReadInstrument { connection_id, .. }
        | ProcessAction::WriteInstrument { connection_id, .. }
        | ProcessAction::DescribeVirtualInstruments { connection_id } => Some(*connection_id),

        ProcessAction::StartAcquisition
        | ProcessAction::StopAcquisition
        | ProcessAction::ClearSeries
        | ProcessAction::StartEmulator
        | ProcessAction::StopEmulator
        | ProcessAction::AddFilteredSeries { .. }
        | ProcessAction::SetFilter { .. }
        | ProcessAction::WriteControllerParameter { .. }
        | ProcessAction::ConfigureController { .. }
        | ProcessAction::WriteControllerReferenceParameter { .. }
        | ProcessAction::ConfigureControllerReference { .. }
        | ProcessAction::SetControllerReference { .. }
        | ProcessAction::SetControllerInput { .. }
        | ProcessAction::PauseController { .. }
        | ProcessAction::RemoveController { .. }
        | ProcessAction::ResumeController { .. }
        | ProcessAction::ResetControllerIntegral { .. }
        | ProcessAction::ResetController { .. }
        | ProcessAction::DeleteSeriesByName { .. }
        | ProcessAction::RenameSeries { .. }
        | ProcessAction::SetSeriesVisibility { .. }
        | ProcessAction::SetSeriesColor { .. } => None,
    };

    connection_id.map(|connection_id| connection_id.value().to_string())
}

fn action_series_id(action: &ProcessAction) -> Option<String> {
    let series_id = match action {
        ProcessAction::SetFilter { series_id, .. }
        | ProcessAction::DeleteSeriesByName { series_id, .. }
        | ProcessAction::RenameSeries { series_id, .. }
        | ProcessAction::SetSeriesColor { series_id, .. } => *series_id,

        ProcessAction::SetSeriesVisibility { series_id, .. } => Some(*series_id),

        _ => None,
    };

    series_id.map(|series_id| series_id.to_string())
}

fn action_series_name(action: &ProcessAction) -> Option<&str> {
    match action {
        ProcessAction::AddSeries { name, .. } => name.as_deref(),

        ProcessAction::AddFilteredSeries { name, .. }
        | ProcessAction::SetFilter { name, .. }
        | ProcessAction::DeleteSeriesByName { name, .. }
        | ProcessAction::SetSeriesColor { name, .. } => Some(name),

        ProcessAction::RenameSeries { current_name, .. } => Some(current_name),

        ProcessAction::SetSeriesVisibility { series_name, .. } => series_name.as_deref(),

        _ => None,
    }
}

fn sqlite_action_id(action_id: ProcessActionId) -> Result<i64, ProcessRecorderError> {
    i64::try_from(action_id.value()).map_err(|_| {
        ProcessRecorderError::new(format!(
            "Process action id {} exceeds SQLite INTEGER range",
            action_id.value(),
        ))
    })
}

fn recorder_error(context: impl Display, error: impl Display) -> ProcessRecorderError {
    ProcessRecorderError::new(format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{connection::ConnectionId, data::SeriesId};

    #[test]
    fn stores_logs_measurements_and_control_outputs() {
        let path = temporary_database_path();

        let mut writer = SqliteProcessRecordWriter::create(&path).unwrap();

        writer
            .write(ProcessRecord::Log {
                timestamp: UNIX_EPOCH,
                level: ProcessLogLevel::Info,
                message: "Application started".to_owned(),
            })
            .unwrap();

        writer
            .write(ProcessRecord::Measurements {
                measurements: vec![ProcessMeasurement {
                    connection_id: ConnectionId::PRIMARY,
                    series_id: SeriesId::new(1),
                    series_name: "temperature".to_owned(),
                    timestamp: 123.5,
                    value: 42.25,
                }],
            })
            .unwrap();

        writer
            .write(ProcessRecord::ControlOutput {
                output: ProcessControlOutput {
                    timestamp: 124.0,

                    loop_name: "heater".to_owned(),

                    controller_kind: "pid".to_owned(),

                    input_series_id: SeriesId::new(1),

                    connection_id: ConnectionId::PRIMARY,

                    setpoint: Some(100.0),

                    measurement: 80.0,

                    requested_output: 40.0,

                    actual_output: Some(40.0),

                    unconstrained_output: Some(40.0),

                    proportional: Some(40.0),

                    integral: Some(0.0),

                    derivative: Some(0.0),

                    saturated: Some(false),
                },
            })
            .unwrap();

        drop(writer);

        let connection = Connection::open(&path).unwrap();

        let log_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM logs", [], |row| row.get(0))
            .unwrap();

        let measurement: (String, f64, f64) = connection
            .query_row(
                "
                SELECT series_name, timestamp, value
                FROM measurements
                ",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        let control_output: (String, String, f64, f64, Option<f64>, Option<bool>) = connection
            .query_row(
                "
                    SELECT
                        loop_name,
                        controller_kind,
                        measurement,
                        requested_output,
                        actual_output,
                        saturated
                    FROM control_outputs
                    ",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();

        drop(connection);

        let _ = fs::remove_file(&path);

        assert_eq!(log_count, 1);
        assert_eq!(measurement, ("temperature".to_owned(), 123.5, 42.25));
        assert_eq!(
            control_output,
            (
                "heater".to_owned(),
                "pid".to_owned(),
                80.0,
                40.0,
                Some(40.0),
                Some(false),
            ),
        );
    }

    #[test]
    fn stores_action_series_identity() {
        let path = temporary_database_path();

        let mut writer = SqliteProcessRecordWriter::create(&path).unwrap();

        writer
            .write(ProcessRecord::ActionRequested {
                action_id: ProcessActionId::new(1),
                timestamp: UNIX_EPOCH,
                origin: ProcessActionOrigin::UserInterface,
                action: ProcessAction::SetSeriesVisibility {
                    series_id: SeriesId::new(17),
                    series_name: Some("temperature_filtered".to_owned()),
                    visible: false,
                },
            })
            .unwrap();

        drop(writer);

        let connection = Connection::open(&path).unwrap();

        let identity: (String, String) = connection
            .query_row(
                "
                    SELECT
                        series_id,
                        series_name
                    FROM actions
                    ",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        drop(connection);

        let _ = fs::remove_file(&path);

        assert_eq!(
            identity,
            (
                SeriesId::new(17).to_string(),
                "temperature_filtered".to_owned(),
            ),
        );
    }

    #[test]
    fn updates_action_lifecycle() {
        let path = temporary_database_path();

        let mut writer = SqliteProcessRecordWriter::create(&path).unwrap();

        let action_id = ProcessActionId::new(7);

        writer
            .write(ProcessRecord::ActionRequested {
                action_id,
                timestamp: UNIX_EPOCH,
                origin: ProcessActionOrigin::UserInterface,
                action: ProcessAction::DeleteSeriesByName {
                    series_id: Some(SeriesId::new(17)),
                    name: "temperature".to_owned(),
                },
            })
            .unwrap();

        writer
            .write(ProcessRecord::ActionApplied {
                action_id,
                timestamp: UNIX_EPOCH + std::time::Duration::from_secs(1),
                series_id: Some(SeriesId::new(17)),
                series_name: Some("temperature".to_owned()),
                result: None,
            })
            .unwrap();

        drop(writer);

        let connection = Connection::open(&path).unwrap();

        let row: (String, Option<f64>, String, String) = connection
            .query_row(
                "
                SELECT
                    status,
                    completed_at,
                    series_id,
                    series_name
                FROM actions
                WHERE id = 7
                ",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();

        drop(connection);

        let _ = fs::remove_file(&path);

        assert_eq!(
            row,
            (
                "applied".to_owned(),
                Some(1.0),
                SeriesId::new(17).to_string(),
                "temperature".to_owned(),
            ),
        );
    }

    #[test]
    fn records_failed_action() {
        let path = temporary_database_path();

        let mut writer = SqliteProcessRecordWriter::create(&path).unwrap();

        let action_id = ProcessActionId::new(3);

        writer
            .write(ProcessRecord::ActionRequested {
                action_id,
                timestamp: UNIX_EPOCH,
                origin: ProcessActionOrigin::Lua,
                action: ProcessAction::DeleteSeriesByName {
                    series_id: None,
                    name: "missing".to_owned(),
                },
            })
            .unwrap();

        writer
            .write(ProcessRecord::ActionFailed {
                action_id,
                timestamp: UNIX_EPOCH,
                error: "Series 'missing' not found".to_owned(),
            })
            .unwrap();

        drop(writer);

        let connection = Connection::open(&path).unwrap();

        let row: (String, String) = connection
            .query_row(
                "
                    SELECT
                        status,
                        error
                    FROM actions
                    WHERE id = 3
                    ",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        drop(connection);

        let _ = fs::remove_file(&path);

        assert_eq!(
            row,
            ("failed".to_owned(), "Series 'missing' not found".to_owned(),),
        );
    }

    #[test]
    fn stores_requested_action_status() {
        let path = temporary_database_path();

        let mut writer = SqliteProcessRecordWriter::create(&path).unwrap();

        writer
            .write(ProcessRecord::ActionRequested {
                action_id: ProcessActionId::new(5),
                timestamp: UNIX_EPOCH,
                origin: ProcessActionOrigin::UserInterface,
                action: ProcessAction::DeleteSeriesByName {
                    series_id: None,
                    name: "temperature".to_owned(),
                },
            })
            .unwrap();

        drop(writer);

        let connection = Connection::open(&path).unwrap();

        let row: (String, Option<f64>, Option<String>) = connection
            .query_row(
                "
                SELECT
                    status,
                    completed_at,
                    error
                FROM actions
                WHERE id = 5
                ",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        drop(connection);

        let _ = fs::remove_file(&path);

        assert_eq!(row, ("requested".to_owned(), None, None,),);
    }

    #[test]
    fn documented_sql_queries_match_the_actual_schema() {
        let path = temporary_database_path();
        let writer = SqliteProcessRecordWriter::create(&path).unwrap();
        let guide = include_str!("../../docs/process-recording.md").replace("\r\n", "\n");
        for query in crate::lua_api::documentation_tests::fenced_blocks(&guide, "sql") {
            writer
                .connection
                .prepare(query)
                .unwrap_or_else(|error| panic!("{query}: {error}"));
        }
        drop(writer);
        fs::remove_file(path).unwrap();
    }

    fn temporary_database_path() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir().join(format!(
            "com_port_reader_process_test_{}_{}.sqlite3",
            std::process::id(),
            unique,
        ))
    }

    #[test]
    fn exposes_chronological_process_timeline() {
        let path = temporary_database_path();

        let mut writer = SqliteProcessRecordWriter::create(&path).unwrap();

        let action_id = ProcessActionId::new(7);

        writer
            .write(ProcessRecord::Log {
                timestamp: UNIX_EPOCH + std::time::Duration::from_secs(1),
                level: ProcessLogLevel::Info,
                message: "test log".to_owned(),
            })
            .unwrap();

        writer
            .write(ProcessRecord::ActionRequested {
                action_id,
                timestamp: UNIX_EPOCH + std::time::Duration::from_secs(2),
                origin: ProcessActionOrigin::UserInterface,
                action: ProcessAction::StartAcquisition,
            })
            .unwrap();

        writer
            .write(ProcessRecord::ActionApplied {
                action_id,
                timestamp: UNIX_EPOCH + std::time::Duration::from_secs(3),
                series_id: None,
                series_name: None,
                result: None,
            })
            .unwrap();

        drop(writer);

        let connection = Connection::open(&path).unwrap();

        let mut statement = connection
            .prepare(
                "
                SELECT event_type
                FROM process_timeline
                ORDER BY timestamp
                ",
            )
            .unwrap();

        let events = statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        drop(statement);
        drop(connection);

        let _ = fs::remove_file(&path);

        assert_eq!(events, vec!["log", "action_requested", "action_applied",],);
    }
}
