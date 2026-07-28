use crate::{
    data::{Sample, SeriesId, SeriesMetadata, SeriesSample, SeriesSource},
    protocol::metakon::{ReadRegisterRequest, read_int_register},
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
            let config = self
                .config_store
                .snapshot()
                .ok_or_else(|| AcquisitionError::from("COM port is not selected"))?;

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
        output: &mut Vec<SeriesSample>,
    ) -> Result<(), AcquisitionError> {
        let has_com_series = series.iter().any(|series| {
            matches!(
                &series.source,
                SeriesSource::SerialCommand { .. } | SeriesSource::Metakon { .. }
            )
        });

        if !has_com_series {
            return Ok(());
        }

        let connection = self.connection().map_err(|error| {
            AcquisitionError::from(format!("Cannot acquire serial series: {error}",))
        })?;

        for series in series {
            let value = match &series.source {
                SeriesSource::Generated(_) => {
                    continue;
                }

                SeriesSource::SerialCommand { command, step } => {
                    let request = serial_request(series.id, command, *step);

                    connection.request_f64(&request).map_err(|error| {
                        AcquisitionError::from(format!(
                            "COM series '{}': request \
                                 '{}' failed: {error}",
                            series.name, request,
                        ))
                    })?
                }

                SeriesSource::Metakon {
                    device,
                    channel,
                    register,
                    scale,
                } => {
                    let request = ReadRegisterRequest::new(*device, *channel, *register);

                    let raw_value = read_int_register(connection, request).map_err(|error| {
                        AcquisitionError::from(format!(
                            "Metakon series '{}': device {}, \
                                     channel {}, register 0x{:02X} \
                                     failed: {error}",
                            series.name, device, channel, register,
                        ))
                    })?;

                    if raw_value == i16::MIN {
                        return Err(AcquisitionError::from(format!(
                            "Metakon series '{}': instrument \
                                 reported alarm value -32768",
                            series.name,
                        )));
                    }

                    f64::from(raw_value) * *scale
                }
            };

            output.push(SeriesSample::new(series.id, Sample::new(timestamp, value)));
        }

        Ok(())
    }

    fn request_text(&mut self, command: &str) -> Result<Option<String>, AcquisitionError> {
        let connection = self.connection().map_err(|error| {
            AcquisitionError::from(format!(
                "Cannot send COM command \
                     '{command}': {error}",
            ))
        })?;

        let response = connection.request_text(command).map_err(|error| {
            AcquisitionError::from(format!("COM command '{command}' failed: {error}",))
        })?;

        Ok(Some(response))
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

        source.sample(&series, 1_000.0, &mut output).unwrap();

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

        let error = source.sample(&series, 1_000.0, &mut output).unwrap_err();

        assert_eq!(
            error.to_string(),
            "Cannot acquire serial series: \
             COM port is not selected",
        );

        assert!(output.is_empty());
    }

    #[test]
    fn formats_keyed_walk_request() {
        let request = serial_request(SeriesId::new(42), "walk", 0.25);

        assert_eq!(request, "walk 42 0.25");
    }

    #[test]
    fn normalizes_walk_command_case() {
        let request = serial_request(SeriesId::new(7), "WALK", 2.0);

        assert_eq!(request, "walk 7 2");
    }

    #[test]
    fn preserves_other_serial_commands() {
        let request = serial_request(SeriesId::new(42), "status", 5.0);

        assert_eq!(request, "status");
    }

    #[test]
    fn reports_missing_config_for_metakon_series() {
        let mut source = SerialCommandSource::new(SerialConfigStore::new());

        let series = vec![SeriesMetadata {
            id: SeriesId::new(1),
            name: "temperature".to_owned(),

            source: SeriesSource::Metakon {
                device: 1,
                channel: 0,
                register: 0x01,
                scale: 0.1,
            },

            visible: true,
        }];

        let mut output = Vec::new();

        let error = source.sample(&series, 1_000.0, &mut output).unwrap_err();

        assert_eq!(
            error.to_string(),
            "Cannot acquire serial series: \
             COM port is not selected",
        );

        assert!(output.is_empty());
    }
}
