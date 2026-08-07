use std::collections::HashSet;

use crate::{
    connection::ConnectionId,
    data::{Sample, SeriesMetadata, SeriesSource},
    instrument::{
        InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest,
        metakon_5x3::{Metakon5x3, Metakon5x3Register},
        virtual_instrument::VirtualInstrumentDescriptor,
    },
    protocol::{metakon::RegisterValue, virtual_instrument::VirtualInstrumentClient},
    serial_connection::{SerialConfigStore, SerialConnection},
    utils::current_time_f64,
};

use super::{AcquisitionError, AcquisitionSource};

pub struct SerialCommandSource {
    connection_id: ConnectionId,
    config_store: SerialConfigStore,
    connection: Option<SerialConnection>,
    verified_metakon_channels: HashSet<(u8, u8)>,
}

impl SerialCommandSource {
    pub fn new(config_store: SerialConfigStore) -> Self {
        Self::for_connection(ConnectionId::PRIMARY, config_store)
    }

    pub fn for_connection(connection_id: ConnectionId, config_store: SerialConfigStore) -> Self {
        Self {
            connection_id,
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
    ) -> Result<Option<InstrumentValue>, AcquisitionError> {
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

                Ok(Some(scale_instrument_value(value, scale)))
            }

            InstrumentReadRequest::VirtualInstrument {
                instrument,
                parameter,
            } => {
                let connection = self.connection()?;

                let mut client = VirtualInstrumentClient::new(connection);

                let value = client
                    .read(instrument, parameter)
                    .map_err(|error| AcquisitionError::from(error.to_string()))?;

                Ok(Some(value))
            }
        }
    }

    fn write_instrument_value(
        &mut self,
        request: InstrumentWriteRequest,
    ) -> Result<Option<InstrumentValue>, AcquisitionError> {
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

                Ok(Some(scale_instrument_value(actual_value, scale)))
            }

            InstrumentWriteRequest::VirtualInstrument {
                instrument,
                parameter,
                value,
            } => {
                let connection = self.connection()?;

                let mut client = VirtualInstrumentClient::new(connection);

                let actual_value = client
                    .write(instrument, parameter, value)
                    .map_err(|error| AcquisitionError::from(error.to_string()))?;

                Ok(Some(actual_value))
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
    fn sample_series(
        &mut self,
        series: &SeriesMetadata,
    ) -> Result<Option<Sample>, AcquisitionError> {
        if series.connection_id != self.connection_id {
            return Ok(None);
        }
        let value = match &series.source {
            SeriesSource::SerialCommand { command } => {
                let connection = self.connection().map_err(|error| {
                    AcquisitionError::from(format!(
                        "Cannot acquire serial series: \
                             {error}",
                    ))
                })?;

                connection.request_f64(command).map_err(|error| {
                    AcquisitionError::from(format!(
                        "COM series '{}': request '{}' \
                             failed: {error}",
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

                let Some(value) = value else {
                    return Ok(None);
                };

                value.as_f64()
            }
        };

        let timestamp = current_time_f64();

        Ok(Some(Sample::new(timestamp, value)))
    }

    fn describe_virtual_instruments(
        &mut self,
    ) -> Result<Option<Vec<VirtualInstrumentDescriptor>>, AcquisitionError> {
        let connection = self.connection().map_err(|error| {
            AcquisitionError::from(format!("Cannot describe virtual instruments: {error}",))
        })?;

        let mut client = VirtualInstrumentClient::new(connection);

        let descriptors = client.describe().map_err(|error| {
            AcquisitionError::from(format!("Failed to describe virtual instruments: {error}",))
        })?;

        Ok(Some(descriptors))
    }

    fn read_instrument(
        &mut self,
        request: InstrumentReadRequest,
    ) -> Result<Option<InstrumentValue>, AcquisitionError> {
        self.read_instrument_value(request)
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
        self.write_instrument_value(request)
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
        connection::ConnectionId,
        data::{SeriesId, SeriesMetadata, SeriesSource},
        instrument::{
            InstrumentReadRequest,
            metakon_5x3::{Metakon5x3, Metakon5x3IdentificationError, Metakon5x3Register},
            virtual_instrument::{VirtualInstrumentId, VirtualParameterId},
        },
        serial_connection::SerialConfigStore,
    };

    #[test]
    fn accepts_empty_series_without_config() {
        let mut source = SerialCommandSource::new(SerialConfigStore::new());

        let mut output = Vec::new();
        let mut failures = Vec::new();

        source.sample(&[], &mut output, &mut failures);

        assert!(output.is_empty());
        assert!(failures.is_empty());
    }

    #[test]
    fn reports_missing_serial_config() {
        let mut source = SerialCommandSource::new(SerialConfigStore::new());

        let series = vec![SeriesMetadata {
            id: SeriesId::new(1),
            connection_id: ConnectionId::PRIMARY,
            name: "random_walk".to_owned(),

            source: SeriesSource::SerialCommand {
                command: "read walue".to_owned(),
            },
            sampling_interval: None,
            visible: true,
        }];

        let mut output = Vec::new();
        let mut failures = Vec::new();

        source.sample(&series, &mut output, &mut failures);

        assert!(output.is_empty());
        assert_eq!(failures.len(), 1);

        assert_eq!(failures[0].series_id, SeriesId::new(1));
        assert_eq!(failures[0].series_name, "random_walk");

        assert_eq!(
            failures[0].error.to_string(),
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
            connection_id: ConnectionId::PRIMARY,
            name: "temperature".to_owned(),

            source: SeriesSource::Instrument(InstrumentReadRequest::metakon_5x3(
                Metakon5x3::new(1, 0),
                Metakon5x3Register::Measurement,
                0.1,
            )),
            sampling_interval: None,
            visible: true,
        }];

        let mut output = Vec::new();
        let mut failures = Vec::new();

        source.sample(&series, &mut output, &mut failures);

        assert!(output.is_empty());
        assert_eq!(failures.len(), 1);

        assert_eq!(failures[0].series_id, SeriesId::new(1));
        assert_eq!(failures[0].series_name, "temperature");

        assert_eq!(
            failures[0].error.to_string(),
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

    #[test]
    fn reports_missing_config_for_virtual_series() {
        let mut source = SerialCommandSource::new(SerialConfigStore::new());

        let request = InstrumentReadRequest::virtual_instrument(
            VirtualInstrumentId::new(1),
            VirtualParameterId::new(1),
        );

        let series = vec![SeriesMetadata {
            id: SeriesId::new(1),
            connection_id: ConnectionId::PRIMARY,
            name: "signal".to_owned(),
            source: SeriesSource::Instrument(request),
            sampling_interval: None,
            visible: true,
        }];

        let mut output = Vec::new();
        let mut failures = Vec::new();

        source.sample(&series, &mut output, &mut failures);

        assert!(output.is_empty());
        assert_eq!(failures.len(), 1);

        assert_eq!(failures[0].series_id, SeriesId::new(1));
        assert_eq!(failures[0].series_name, "signal");

        assert_eq!(
            failures[0].error.to_string(),
            "Virtual instrument series 'signal': \
             COM port is not selected",
        );

        assert!(output.is_empty());
    }

    #[test]
    fn reports_missing_config_when_describing_virtual_instruments() {
        let mut source = SerialCommandSource::new(SerialConfigStore::new());

        let error = source.describe_virtual_instruments().unwrap_err();

        assert_eq!(
            error.to_string(),
            "Cannot describe virtual instruments: \
             COM port is not selected",
        );
    }

    #[test]
    fn ignores_series_from_another_connection() {
        let mut source =
            SerialCommandSource::for_connection(ConnectionId::new(1), SerialConfigStore::new());

        let series = SeriesMetadata {
            id: SeriesId::new(1),
            connection_id: ConnectionId::new(2),
            name: "foreign".to_owned(),

            source: SeriesSource::SerialCommand {
                command: "read value".to_owned(),
            },

            visible: true,
            sampling_interval: None,
        };

        let result = source.sample_series(&series).unwrap();

        assert_eq!(result, None);
    }
}
