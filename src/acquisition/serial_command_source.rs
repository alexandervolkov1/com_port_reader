use std::collections::HashSet;

use crate::{
    data::{Sample, SeriesMetadata, SeriesSample, SeriesSource},
    instrument::{
        InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest,
        metakon_5x3::{Metakon5x3, Metakon5x3Register},
    },
    protocol::metakon::RegisterValue,
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

    fn read_instrument_value(
        &mut self,
        request: InstrumentReadRequest,
    ) -> Result<InstrumentValue, AcquisitionError> {
        match request {
            InstrumentReadRequest::Metakon5x3 {
                instrument,
                parameter,
                scale,
            } => {
                if !scale.is_finite() || scale <= 0.0 {
                    return Err(AcquisitionError::from(
                        "Instrument scale must be finite \
                             and greater than zero",
                    ));
                }

                let connection =
                    self.verified_metakon_connection(instrument.device(), instrument.channel())?;

                let register_value = instrument
                    .read(connection, parameter)
                    .map_err(|error| AcquisitionError::from(error.to_string()))?;

                if parameter == Metakon5x3Register::Measurement
                    && matches!(
                        &register_value,
                        RegisterValue::Int(value)
                            if *value == i16::MIN
                    )
                {
                    return Err(AcquisitionError::from(
                        "instrument reported alarm \
                             value -32768",
                    ));
                }

                let value = register_value_to_instrument_value(register_value)?;

                Ok(scale_instrument_value(value, scale))
            }
        }
    }

    fn write_instrument_value(
        &mut self,
        request: InstrumentWriteRequest,
    ) -> Result<InstrumentValue, AcquisitionError> {
        match request {
            InstrumentWriteRequest::Metakon5x3 {
                instrument,
                parameter,
                scale,
            } => {
                if !scale.is_finite() || scale <= 0.0 {
                    return Err(AcquisitionError::from(
                        "Instrument scale must be finite \
                         and greater than zero",
                    ));
                }

                let connection =
                    self.verified_metakon_connection(instrument.device(), instrument.channel())?;

                let actual_value = instrument
                    .write(connection, parameter)
                    .map_err(|error| AcquisitionError::from(error.to_string()))?;

                let actual_value = register_value_to_instrument_value(actual_value)?;

                Ok(scale_instrument_value(actual_value, scale))
            }
        }
    }
}

fn register_value_to_instrument_value(
    value: RegisterValue,
) -> Result<InstrumentValue, AcquisitionError> {
    match value {
        RegisterValue::Bool(value) => Ok(InstrumentValue::Boolean(value)),

        RegisterValue::Ubyte(value) => Ok(InstrumentValue::Integer(i64::from(value))),

        RegisterValue::Byte(value) => Ok(InstrumentValue::Integer(i64::from(value))),

        RegisterValue::Uint(value) => Ok(InstrumentValue::Integer(i64::from(value))),

        RegisterValue::Int(value) => Ok(InstrumentValue::Integer(i64::from(value))),

        RegisterValue::Ulong(value) => Ok(InstrumentValue::Integer(i64::from(value))),

        RegisterValue::Long(value) => Ok(InstrumentValue::Integer(i64::from(value))),

        RegisterValue::Float(value) => Ok(InstrumentValue::Number(f64::from(value))),

        RegisterValue::Double(value) => Ok(InstrumentValue::Number(value)),

        RegisterValue::Ascii(_) => Err(AcquisitionError::from(
            "Instrument operation returned an \
                 unexpected ASCII value",
        )),
    }
}

fn scale_instrument_value(value: InstrumentValue, scale: f64) -> InstrumentValue {
    match value {
        InstrumentValue::Boolean(value) => InstrumentValue::Boolean(value),

        InstrumentValue::Integer(value) if scale == 1.0 => InstrumentValue::Integer(value),

        InstrumentValue::Integer(value) => InstrumentValue::Number(value as f64 * scale),

        InstrumentValue::Number(value) => InstrumentValue::Number(value * scale),
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

                SeriesSource::Instrument(request) => {
                    let value = self.read_instrument_value(*request).map_err(|error| {
                        AcquisitionError::from(format!(
                            "{} series '{}': {error}",
                            request.kind_name(),
                            series.name,
                        ))
                    })?;

                    value.as_f64()
                }
            };

            output.push(SeriesSample::new(series.id, Sample::new(timestamp, value)));
        }

        Ok(())
    }

    fn read_instrument(
        &mut self,
        request: InstrumentReadRequest,
    ) -> Result<Option<InstrumentValue>, AcquisitionError> {
        self.read_instrument_value(request).map(Some)
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

    fn write_instrument(
        &mut self,
        request: InstrumentWriteRequest,
    ) -> Result<Option<InstrumentValue>, AcquisitionError> {
        self.write_instrument_value(request).map(Some)
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
        data::{SeriesId, SeriesMetadata, SeriesSource},
        instrument::{
            InstrumentReadRequest,
            metakon_5x3::{Metakon5x3, Metakon5x3IdentificationError, Metakon5x3Register},
        },
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

            source: SeriesSource::Instrument(InstrumentReadRequest::metakon_5x3(
                Metakon5x3::new(1, 0),
                Metakon5x3Register::Measurement,
                0.1,
            )),

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
