use std::collections::HashSet;

use crate::{
    data::{MetakonValueType, Sample, SeriesMetadata, SeriesSample, SeriesSource},
    instrument::metakon_5x3::{Metakon5x3, Metakon5x3Register, Metakon5x3Write},
    protocol::metakon::{RegisterDataType, RegisterValue, WriteRegisterRequest},
    serial_connection::{SerialConfigStore, SerialConnection},
};

use super::{AcquisitionError, AcquisitionSource};

pub struct SerialCommandSource {
    config_store: SerialConfigStore,
    connection: Option<SerialConnection>,
    verified_metakon_channels: HashSet<(u8, u8)>,
}

impl SerialCommandSource {
    pub fn new(config_store: SerialConfigStore) -> Self {
        Self {
            config_store,
            connection: None,
            verified_metakon_channels: HashSet::new(),
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

            self.verified_metakon_channels.clear();
            self.connection = Some(connection);
        }

        Ok(self
            .connection
            .as_mut()
            .expect("connection was initialized above"))
    }

    fn verified_metakon_connection(
        &mut self,
        device: u8,
        channel: u8,
    ) -> Result<&mut SerialConnection, AcquisitionError> {
        let address = (device, channel);

        if !self.verified_metakon_channels.contains(&address) {
            let instrument = Metakon5x3::new(device, channel);

            {
                let connection = self.connection()?;

                instrument
                    .verify_channel_type(connection)
                    .map_err(|error| {
                        AcquisitionError::from(format!(
                            "Metakon device {device}, channel \
                             {channel} is not a Metakon 5X3 \
                             channel: {error}",
                        ))
                    })?;
            }

            self.verified_metakon_channels.insert(address);
        }

        self.connection()
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

        for series in series {
            let value = match &series.source {
                SeriesSource::SerialCommand { command } => {
                    let connection = self.connection().map_err(|error| {
                        AcquisitionError::from(format!("Cannot acquire serial series: {error}",))
                    })?;

                    connection.request_f64(command).map_err(|error| {
                        AcquisitionError::from(format!(
                            "COM series '{}': request '{}' failed: \
                             {error}",
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

                    let connection = self
                        .verified_metakon_connection(*device, *channel)
                        .map_err(|error| {
                            AcquisitionError::from(format!(
                                "Metakon series '{}': {error}",
                                series.name,
                            ))
                        })?;

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
        let connection = self
            .verified_metakon_connection(request.device(), request.channel())
            .map_err(|error| {
                AcquisitionError::from(format!("Cannot write Metakon register: {error}",))
            })?;

        let instrument = Metakon5x3::new(request.device(), request.channel());

        let parameter =
            Metakon5x3Write::try_from((request.register(), request.value())).map_err(|error| {
                AcquisitionError::from(format!("Cannot write Metakon 5X3 register: {error}",))
            })?;

        let actual_value = instrument
            .write(connection, parameter)
            .map_err(|error| AcquisitionError::from(error.to_string()))?;

        Ok(Some(actual_value))
    }

    fn stop(&mut self) -> Result<(), AcquisitionError> {
        self.connection.take();
        self.verified_metakon_channels.clear();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{AcquisitionSource, SerialCommandSource};

    use crate::{
        data::{MetakonValueType, SeriesId, SeriesMetadata, SeriesSource},
        instrument::metakon_5x3::Metakon5x3IdentificationError,
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
            "Metakon series 'temperature': \
             COM port is not selected",
        );

        assert!(output.is_empty());
    }

    #[test]
    fn reports_unexpected_channel_type() {
        let error = Metakon5x3IdentificationError::UnexpectedChannelType {
            expected: 0x03,
            actual: 0x04,
        };

        assert_eq!(
            error.to_string(),
            "unexpected Metakon channel type 0x04; \
             expected Metakon 5X3 type 0x03",
        );
    }
}
