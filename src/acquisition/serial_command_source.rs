use crate::{
    data::{MetakonValueType, Sample, SeriesMetadata, SeriesSample, SeriesSource},
    instrument::metakon_5x3::{Metakon5x3, Metakon5x3Register},
    protocol::metakon::{
        ReadRegisterRequest, RegisterDataType, RegisterValue, WriteRegisterRequest, read_register,
        write_register,
    },
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

impl AcquisitionSource for SerialCommandSource {
    fn sample(
        &mut self,
        series: &[SeriesMetadata],
        timestamp: f64,
        output: &mut Vec<SeriesSample>,
    ) -> Result<(), AcquisitionError> {
        if series.is_empty() {
            return Ok(());
        }

        let connection = self.connection().map_err(|error| {
            AcquisitionError::from(format!("Cannot acquire serial series: {error}",))
        })?;

        for series in series {
            let value = match &series.source {
                SeriesSource::SerialCommand { command } => {
                    connection.request_f64(command).map_err(|error| {
                        AcquisitionError::from(format!(
                            "COM series '{}': request \
                                 '{}' failed: {error}",
                            series.name, command,
                        ))
                    })?
                }

                SeriesSource::Metakon {
                    device,
                    channel,
                    register,
                    value_type,
                    scale,
                } => {
                    let Some(typed_register) = Metakon5x3Register::from_address(*register) else {
                        return Err(AcquisitionError::from(format!(
                            "Metakon series '{}': register \
                             0x{register:02X} does not belong to \
                             the Metakon 5X3 register model",
                            series.name,
                        )));
                    };

                    let configured_type = match value_type {
                        MetakonValueType::Ubyte => RegisterDataType::Ubyte,

                        MetakonValueType::Byte => RegisterDataType::Byte,

                        MetakonValueType::Uint => RegisterDataType::Uint,

                        MetakonValueType::Int => RegisterDataType::Int,
                    };

                    let expected_type = typed_register.data_type();

                    if configured_type != expected_type {
                        return Err(AcquisitionError::from(format!(
                            "Metakon series '{}': register \
                             0x{register:02X} has type {:?}, \
                             but {:?} was configured",
                            series.name, expected_type, configured_type,
                        )));
                    }

                    let instrument = Metakon5x3::new(*device, *channel);

                    let register_value =
                        instrument
                            .read(connection, typed_register)
                            .map_err(|error| {
                                AcquisitionError::from(format!(
                                    "Metakon series '{}': {error}",
                                    series.name,
                                ))
                            })?;

                    let raw_value = register_value.into_f64().expect(
                        "Metakon 5X3 registers always contain \
                             numeric or boolean values",
                    );

                    if typed_register == Metakon5x3Register::Measurement
                        && raw_value == f64::from(i16::MIN)
                    {
                        return Err(AcquisitionError::from(format!(
                            "Metakon series '{}': instrument \
                             reported alarm value -32768",
                            series.name,
                        )));
                    }

                    raw_value * *scale
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
            AcquisitionError::from(format!(
                "COM command '{command}' failed: \
                     {error}",
            ))
        })?;

        Ok(Some(response))
    }

    fn write_metakon_register(
        &mut self,
        request: WriteRegisterRequest,
    ) -> Result<Option<RegisterValue>, AcquisitionError> {
        let connection = self.connection().map_err(|error| {
            AcquisitionError::from(format!(
                "Cannot write Metakon register: \
                     {error}",
            ))
        })?;

        write_register(connection, request).map_err(|error| {
            AcquisitionError::from(format!(
                "Metakon device {}, channel {}, \
                     register 0x{:02X} write failed: \
                     {error}",
                request.device(),
                request.channel(),
                request.register(),
            ))
        })?;

        let read_request =
            ReadRegisterRequest::new(request.device(), request.channel(), request.register());

        let actual_value = read_register(connection, read_request, request.value().data_type())
            .map_err(|error| {
                AcquisitionError::from(format!(
                    "Metakon device {}, channel {}, register \
                 0x{:02X} was written, but verification \
                 read failed: {error}",
                    request.device(),
                    request.channel(),
                    request.register(),
                ))
            })?;

        Ok(Some(actual_value))
    }

    fn stop(&mut self) -> Result<(), AcquisitionError> {
        self.connection.take();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{AcquisitionSource, SerialCommandSource};

    use crate::{
        data::{MetakonValueType, SeriesId, SeriesMetadata, SeriesSource},
        serial_connection::SerialConfigStore,
    };

    #[test]
    fn accepts_empty_series_without_config() {
        let mut source = SerialCommandSource::new(SerialConfigStore::new());

        let mut output = Vec::new();

        source.sample(&[], 1_000.0, &mut output).unwrap();

        assert!(output.is_empty());
    }

    #[test]
    fn reports_missing_serial_config() {
        let mut source = SerialCommandSource::new(SerialConfigStore::new());

        let series = vec![SeriesMetadata {
            id: SeriesId::new(1),
            name: "random_walk".to_owned(),

            source: SeriesSource::SerialCommand {
                command: "read walue".to_owned(),
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
    fn reports_missing_config_for_metakon_series() {
        let mut source = SerialCommandSource::new(SerialConfigStore::new());

        let series = vec![SeriesMetadata {
            id: SeriesId::new(1),
            name: "temperature".to_owned(),

            source: SeriesSource::Metakon {
                device: 1,
                channel: 0,
                value_type: MetakonValueType::Int,
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
