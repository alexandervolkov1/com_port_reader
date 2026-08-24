use std::{
    fmt::Display,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, params};

use super::{
    ProcessAction, ProcessActionOrigin, ProcessLogLevel, ProcessMeasurement, ProcessRecord,
    ProcessRecordWriter, ProcessRecorderError,
};

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
                    id            INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp     REAL NOT NULL,
                    origin        TEXT NOT NULL,
                    action_type   TEXT NOT NULL,
                    connection_id TEXT,
                    series_id     TEXT,
                    details       TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS measurements (
                    id            INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp     REAL NOT NULL,
                    connection_id TEXT NOT NULL,
                    series_id     TEXT NOT NULL,
                    series_name   TEXT NOT NULL,
                    value         REAL NOT NULL
                );

                CREATE INDEX IF NOT EXISTS measurements_timestamp_index
                    ON measurements(timestamp);

                CREATE INDEX IF NOT EXISTS measurements_series_index
                    ON measurements(series_id, timestamp);

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
        timestamp: SystemTime,
        origin: ProcessActionOrigin,
        action: ProcessAction,
    ) -> Result<(), ProcessRecorderError> {
        let action_type = action_type_name(&action);
        let connection_id = action_connection_id(&action);
        let series_id = action_series_id(&action);
        let details = format!("{action:?}");

        self.connection
            .execute(
                "
                INSERT INTO actions (
                    timestamp,
                    origin,
                    action_type,
                    connection_id,
                    series_id,
                    details
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ",
                params![
                    system_time_seconds(timestamp)?,
                    action_origin_name(origin),
                    action_type,
                    connection_id,
                    series_id,
                    details,
                ],
            )
            .map_err(|error| recorder_error("Failed to write action record", error))?;

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
                timestamp,
                origin,
                action,
            } => self.write_action(timestamp, origin, action),

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

        ProcessActionOrigin::ProcessControl => "process_control",
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
        | ProcessAction::DeleteSeriesByName { .. }
        | ProcessAction::RenameSeries { .. }
        | ProcessAction::SetSeriesVisibility { .. }
        | ProcessAction::SetSeriesColor { .. } => None,
    };

    connection_id.map(|connection_id| connection_id.value().to_string())
}

fn action_series_id(action: &ProcessAction) -> Option<String> {
    match action {
        ProcessAction::SetSeriesVisibility { series_id, .. } => Some(series_id.to_string()),

        _ => None,
    }
}

fn recorder_error(context: impl Display, error: impl Display) -> ProcessRecorderError {
    ProcessRecorderError::new(format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{connection::ConnectionId, data::SeriesId};

    #[test]
    fn stores_logs_and_measurements() {
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

        drop(connection);

        let _ = fs::remove_file(&path);

        assert_eq!(log_count, 1);
        assert_eq!(measurement, ("temperature".to_owned(), 123.5, 42.25));
    }

    #[test]
    fn identifies_process_control_action_origin() {
        assert_eq!(
            action_origin_name(ProcessActionOrigin::ProcessControl,),
            "process_control",
        );
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
}
