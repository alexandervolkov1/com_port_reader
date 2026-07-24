use crate::{
    data::{Sample, SeriesId, SeriesMetadata, SeriesSample, SeriesSource},
    serial_connection::{SerialConfigStore, SerialConnection},
};

use super::{AcquisitionError, AcquisitionSource};

pub struct SerialCommandSource {
    config_store: SerialConfigStore,
    connection: Option<SerialConnection>,
}

impl SerialCommandSource {
    pub fn new(config_store: SerialConfigStore) -> Self {
        Self {
            config_store,
            connection: None,
        }
    }

    fn connection(&mut self) -> Result<&mut SerialConnection, AcquisitionError> {
        if self.connection.is_none() {
            let config = self.config_store.snapshot().ok_or_else(|| {
                AcquisitionError::from(
                    "Cannot acquire serial series: \
                         COM port is not selected",
                )
            })?;

            let port_name = config.port_name().to_owned();

            let connection = config.open().map_err(|error| {
                AcquisitionError::from(format!(
                    "Failed to open COM port \
                         '{port_name}': {error}",
                ))
            })?;

            self.connection = Some(connection);
        }

        Ok(self
            .connection
            .as_mut()
            .expect("connection was initialized above"))
    }
}

fn serial_request(id: SeriesId, command: &str, step: f64) -> String {
    if command.eq_ignore_ascii_case("walk") {
        format!("walk {id} {step}")
    } else {
        command.to_owned()
    }
}

impl AcquisitionSource for SerialCommandSource {
    fn sample(
        &mut self,
        series: &[SeriesMetadata],
        timestamp: f64,
        _elapsed_seconds: f64,
        output: &mut Vec<SeriesSample>,
    ) -> Result<(), AcquisitionError> {
        let has_serial_series = series
            .iter()
            .any(|series| matches!(&series.source, SeriesSource::SerialCommand { .. }));

        if !has_serial_series {
            return Ok(());
        }

        let connection = self.connection()?;

        for series in series {
            let SeriesSource::SerialCommand { command, step } = &series.source else {
                continue;
            };

            let request = serial_request(series.id, command, *step);

            let value = connection.request_f64(&request).map_err(|error| {
                AcquisitionError::from(format!(
                    "COM series '{}': request \
                         '{}' failed: {error}",
                    series.name, request,
                ))
            })?;

            output.push(SeriesSample::new(series.id, Sample::new(timestamp, value)));
        }

        Ok(())
    }

    fn stop(&mut self) -> Result<(), AcquisitionError> {
        self.connection.take();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{AcquisitionSource, SerialCommandSource, serial_request};

    use crate::{
        data::{SeriesId, SeriesMetadata, SeriesSource, Signal},
        serial_connection::SerialConfigStore,
    };

    #[test]
    fn ignores_generated_series_without_config() {
        let mut source = SerialCommandSource::new(SerialConfigStore::new());

        let series = vec![SeriesMetadata {
            id: SeriesId::new(1),
            name: "generated".to_owned(),
            source: SeriesSource::Generated(Signal::Constant { value: 10.0 }),
            visible: true,
        }];

        let mut output = Vec::new();

        source.sample(&series, 1_000.0, 5.0, &mut output).unwrap();

        assert!(output.is_empty());
    }

    #[test]
    fn reports_missing_serial_config() {
        let mut source = SerialCommandSource::new(SerialConfigStore::new());

        let series = vec![SeriesMetadata {
            id: SeriesId::new(1),
            name: "random_walk".to_owned(),
            source: SeriesSource::SerialCommand {
                command: "walk".to_owned(),
                step: 1.0,
            },
            visible: true,
        }];

        let mut output = Vec::new();

        let error = source
            .sample(&series, 1_000.0, 5.0, &mut output)
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Cannot acquire serial series: \
             COM port is not selected",
        );

        assert!(output.is_empty());
    }

    #[test]
    fn formats_keyed_get_request() {
        let request = serial_request(SeriesId::new(42), "walk", 0.25);

        assert_eq!(request, "get 42 0.25");
    }

    #[test]
    fn normalizes_get_command_case() {
        let request = serial_request(SeriesId::new(7), "WALK", 2.0);

        assert_eq!(request, "get 7 2");
    }

    #[test]
    fn preserves_other_serial_commands() {
        let request = serial_request(SeriesId::new(42), "status", 5.0);

        assert_eq!(request, "status");
    }
}
