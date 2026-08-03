use std::collections::HashSet;

use crate::instrument::{
    InstrumentValue, ParameterAccess, ParameterRange, ParameterValueType,
    virtual_instrument::{
        VirtualInstrumentDescriptor, VirtualInstrumentId, VirtualInstrumentSchemaError,
        VirtualParameterDescriptor, VirtualParameterId,
    },
};

use super::{MessageKind, VirtualFrameError, VirtualInstrumentFrame};

const ACCESS_READ_ONLY: u8 = 1;
const ACCESS_WRITE_ONLY: u8 = 2;
const ACCESS_READ_WRITE: u8 = 3;

const TYPE_BOOLEAN: u8 = 1;
const TYPE_INTEGER: u8 = 2;
const TYPE_NUMBER: u8 = 3;

const RANGE_NONE: u8 = 0;
const RANGE_INTEGER: u8 = 1;
const RANGE_NUMBER: u8 = 2;

const VALUE_BOOLEAN: u8 = 1;
const VALUE_INTEGER: u8 = 2;
const VALUE_NUMBER: u8 = 3;

#[derive(Clone, Debug, PartialEq)]
pub enum VirtualInstrumentMessage {
    DescribeRequest,

    DescribeResponse {
        instruments: Vec<VirtualInstrumentDescriptor>,
    },

    ReadRequest {
        instrument: VirtualInstrumentId,
        parameter: VirtualParameterId,
    },

    ReadResponse {
        value: InstrumentValue,
    },

    WriteRequest {
        instrument: VirtualInstrumentId,
        parameter: VirtualParameterId,
        value: InstrumentValue,
    },

    WriteResponse {
        value: InstrumentValue,
    },

    ErrorResponse {
        code: u16,
        message: String,
    },
}

impl VirtualInstrumentMessage {
    pub fn encode_frame(&self) -> Result<VirtualInstrumentFrame, VirtualMessageCodecError> {
        let mut encoder = Encoder::new();

        let kind = match self {
            Self::DescribeRequest => MessageKind::DescribeRequest,

            Self::DescribeResponse { instruments } => {
                encode_instrument_catalog(&mut encoder, instruments)?;

                MessageKind::DescribeResponse
            }

            Self::ReadRequest {
                instrument,
                parameter,
            } => {
                encode_parameter_address(&mut encoder, *instrument, *parameter);

                MessageKind::ReadRequest
            }

            Self::ReadResponse { value } => {
                encode_value(&mut encoder, *value);

                MessageKind::ReadResponse
            }

            Self::WriteRequest {
                instrument,
                parameter,
                value,
            } => {
                encode_parameter_address(&mut encoder, *instrument, *parameter);

                encode_value(&mut encoder, *value);

                MessageKind::WriteRequest
            }

            Self::WriteResponse { value } => {
                encode_value(&mut encoder, *value);

                MessageKind::WriteResponse
            }

            Self::ErrorResponse { code, message } => {
                encoder.write_u16(*code);
                encoder.write_string(message)?;

                MessageKind::ErrorResponse
            }
        };

        VirtualInstrumentFrame::new(kind, encoder.finish()).map_err(VirtualMessageCodecError::from)
    }

    pub fn decode_frame(frame: &VirtualInstrumentFrame) -> Result<Self, VirtualMessageCodecError> {
        let mut decoder = Decoder::new(frame.payload());

        let message = match frame.kind() {
            MessageKind::DescribeRequest => Self::DescribeRequest,

            MessageKind::DescribeResponse => Self::DescribeResponse {
                instruments: decode_instrument_catalog(&mut decoder)?,
            },

            MessageKind::ReadRequest => {
                let (instrument, parameter) = decode_parameter_address(&mut decoder)?;

                Self::ReadRequest {
                    instrument,
                    parameter,
                }
            }

            MessageKind::ReadResponse => Self::ReadResponse {
                value: decode_value(&mut decoder)?,
            },

            MessageKind::WriteRequest => {
                let (instrument, parameter) = decode_parameter_address(&mut decoder)?;

                let value = decode_value(&mut decoder)?;

                Self::WriteRequest {
                    instrument,
                    parameter,
                    value,
                }
            }

            MessageKind::WriteResponse => Self::WriteResponse {
                value: decode_value(&mut decoder)?,
            },

            MessageKind::ErrorResponse => Self::ErrorResponse {
                code: decoder.read_u16()?,
                message: decoder.read_string()?,
            },
        };

        decoder.finish()?;

        Ok(message)
    }
}

fn encode_instrument_catalog(
    encoder: &mut Encoder,
    instruments: &[VirtualInstrumentDescriptor],
) -> Result<(), VirtualMessageCodecError> {
    let mut instrument_ids = HashSet::new();

    for instrument in instruments {
        if !instrument_ids.insert(instrument.id()) {
            return Err(VirtualMessageCodecError::DuplicateInstrumentId(
                instrument.id(),
            ));
        }
    }

    encoder.write_count("instruments", instruments.len())?;

    for instrument in instruments {
        encoder.write_u16(instrument.id().value());

        encoder.write_string(instrument.name())?;

        encoder.write_count("parameters", instrument.parameters().len())?;

        for parameter in instrument.parameters() {
            encode_parameter_descriptor(encoder, parameter)?;
        }
    }

    Ok(())
}

fn decode_instrument_catalog(
    decoder: &mut Decoder<'_>,
) -> Result<Vec<VirtualInstrumentDescriptor>, VirtualMessageCodecError> {
    let instrument_count = usize::from(decoder.read_u16()?);

    let mut instruments = Vec::new();
    let mut instrument_ids = HashSet::new();

    for _ in 0..instrument_count {
        let instrument_id = VirtualInstrumentId::new(decoder.read_u16()?);

        if !instrument_ids.insert(instrument_id) {
            return Err(VirtualMessageCodecError::DuplicateInstrumentId(
                instrument_id,
            ));
        }

        let name = decoder.read_string()?;

        let parameter_count = usize::from(decoder.read_u16()?);

        let mut parameters = Vec::new();

        for _ in 0..parameter_count {
            parameters.push(decode_parameter_descriptor(decoder)?);
        }

        instruments.push(VirtualInstrumentDescriptor::new(
            instrument_id,
            name,
            parameters,
        )?);
    }

    Ok(instruments)
}

fn encode_parameter_descriptor(
    encoder: &mut Encoder,
    parameter: &VirtualParameterDescriptor,
) -> Result<(), VirtualMessageCodecError> {
    encoder.write_u16(parameter.id().value());

    encoder.write_string(parameter.key())?;
    encoder.write_string(parameter.name())?;

    encoder.write_u8(encode_access(parameter.access()));

    encoder.write_u8(encode_parameter_type(parameter.value_type()));

    encoder.write_bool(parameter.series());

    match parameter.range() {
        None => {
            encoder.write_u8(RANGE_NONE);
        }

        Some(ParameterRange::Integer { minimum, maximum }) => {
            encoder.write_u8(RANGE_INTEGER);
            encoder.write_i64(minimum);
            encoder.write_i64(maximum);
        }

        Some(ParameterRange::Number { minimum, maximum }) => {
            encoder.write_u8(RANGE_NUMBER);
            encoder.write_f64(minimum);
            encoder.write_f64(maximum);
        }
    }

    match parameter.unit() {
        Some(unit) => {
            encoder.write_bool(true);
            encoder.write_string(unit)?;
        }

        None => {
            encoder.write_bool(false);
        }
    }

    Ok(())
}

fn decode_parameter_descriptor(
    decoder: &mut Decoder<'_>,
) -> Result<VirtualParameterDescriptor, VirtualMessageCodecError> {
    let id = VirtualParameterId::new(decoder.read_u16()?);

    let key = decoder.read_string()?;
    let name = decoder.read_string()?;

    let access = decode_access(decoder.read_u8()?)?;

    let value_type = decode_parameter_type(decoder.read_u8()?)?;

    let series = decoder.read_bool()?;

    let range = match decoder.read_u8()? {
        RANGE_NONE => None,

        RANGE_INTEGER => Some(ParameterRange::Integer {
            minimum: decoder.read_i64()?,
            maximum: decoder.read_i64()?,
        }),

        RANGE_NUMBER => Some(ParameterRange::Number {
            minimum: decoder.read_f64()?,
            maximum: decoder.read_f64()?,
        }),

        value => {
            return Err(VirtualMessageCodecError::UnknownRangeType(value));
        }
    };

    let unit = if decoder.read_bool()? {
        Some(decoder.read_string()?)
    } else {
        None
    };

    let mut descriptor =
        VirtualParameterDescriptor::new(id, key, name, access, value_type).with_series(series);

    if let Some(range) = range {
        descriptor = descriptor.with_range(range);
    }

    if let Some(unit) = unit {
        descriptor = descriptor.with_unit(unit);
    }

    Ok(descriptor)
}

fn encode_parameter_address(
    encoder: &mut Encoder,
    instrument: VirtualInstrumentId,
    parameter: VirtualParameterId,
) {
    encoder.write_u16(instrument.value());
    encoder.write_u16(parameter.value());
}

fn decode_parameter_address(
    decoder: &mut Decoder<'_>,
) -> Result<(VirtualInstrumentId, VirtualParameterId), VirtualMessageCodecError> {
    Ok((
        VirtualInstrumentId::new(decoder.read_u16()?),
        VirtualParameterId::new(decoder.read_u16()?),
    ))
}

fn encode_value(encoder: &mut Encoder, value: InstrumentValue) {
    match value {
        InstrumentValue::Boolean(value) => {
            encoder.write_u8(VALUE_BOOLEAN);
            encoder.write_bool(value);
        }

        InstrumentValue::Integer(value) => {
            encoder.write_u8(VALUE_INTEGER);
            encoder.write_i64(value);
        }

        InstrumentValue::Number(value) => {
            encoder.write_u8(VALUE_NUMBER);
            encoder.write_f64(value);
        }
    }
}

fn decode_value(decoder: &mut Decoder<'_>) -> Result<InstrumentValue, VirtualMessageCodecError> {
    match decoder.read_u8()? {
        VALUE_BOOLEAN => Ok(InstrumentValue::Boolean(decoder.read_bool()?)),

        VALUE_INTEGER => Ok(InstrumentValue::Integer(decoder.read_i64()?)),

        VALUE_NUMBER => {
            let value = decoder.read_f64()?;

            if !value.is_finite() {
                return Err(VirtualMessageCodecError::NonFiniteNumber);
            }

            Ok(InstrumentValue::Number(value))
        }

        value => Err(VirtualMessageCodecError::UnknownValueType(value)),
    }
}

fn encode_access(access: ParameterAccess) -> u8 {
    match access {
        ParameterAccess::ReadOnly => ACCESS_READ_ONLY,

        ParameterAccess::WriteOnly => ACCESS_WRITE_ONLY,

        ParameterAccess::ReadWrite => ACCESS_READ_WRITE,
    }
}

fn decode_access(value: u8) -> Result<ParameterAccess, VirtualMessageCodecError> {
    match value {
        ACCESS_READ_ONLY => Ok(ParameterAccess::ReadOnly),

        ACCESS_WRITE_ONLY => Ok(ParameterAccess::WriteOnly),

        ACCESS_READ_WRITE => Ok(ParameterAccess::ReadWrite),

        _ => Err(VirtualMessageCodecError::UnknownParameterAccess(value)),
    }
}

fn encode_parameter_type(value_type: ParameterValueType) -> u8 {
    match value_type {
        ParameterValueType::Boolean => TYPE_BOOLEAN,
        ParameterValueType::Integer => TYPE_INTEGER,
        ParameterValueType::Number => TYPE_NUMBER,
    }
}

fn decode_parameter_type(value: u8) -> Result<ParameterValueType, VirtualMessageCodecError> {
    match value {
        TYPE_BOOLEAN => Ok(ParameterValueType::Boolean),

        TYPE_INTEGER => Ok(ParameterValueType::Integer),

        TYPE_NUMBER => Ok(ParameterValueType::Number),

        _ => Err(VirtualMessageCodecError::UnknownParameterValueType(value)),
    }
}

struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn write_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn write_bool(&mut self, value: bool) {
        self.write_u8(u8::from(value));
    }

    fn write_u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn write_i64(&mut self, value: i64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn write_f64(&mut self, value: f64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn write_count(
        &mut self,
        collection: &'static str,
        length: usize,
    ) -> Result<(), VirtualMessageCodecError> {
        let length = u16::try_from(length)
            .map_err(|_| VirtualMessageCodecError::CollectionTooLong { collection, length })?;

        self.write_u16(length);

        Ok(())
    }

    fn write_string(&mut self, value: &str) -> Result<(), VirtualMessageCodecError> {
        let bytes = value.as_bytes();

        let length = u16::try_from(bytes.len())
            .map_err(|_| VirtualMessageCodecError::StringTooLong(bytes.len()))?;

        self.write_u16(length);
        self.bytes.extend_from_slice(bytes);

        Ok(())
    }
}

struct Decoder<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Decoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn finish(self) -> Result<(), VirtualMessageCodecError> {
        let remaining = self.bytes.len() - self.position;

        if remaining == 0 {
            Ok(())
        } else {
            Err(VirtualMessageCodecError::TrailingBytes(remaining))
        }
    }

    fn read_bytes(&mut self, length: usize) -> Result<&'a [u8], VirtualMessageCodecError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(VirtualMessageCodecError::UnexpectedEnd)?;

        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or(VirtualMessageCodecError::UnexpectedEnd)?;

        self.position = end;

        Ok(bytes)
    }

    fn read_u8(&mut self) -> Result<u8, VirtualMessageCodecError> {
        Ok(self.read_bytes(1)?[0])
    }

    fn read_bool(&mut self) -> Result<bool, VirtualMessageCodecError> {
        match self.read_u8()? {
            0 => Ok(false),
            1 => Ok(true),

            value => Err(VirtualMessageCodecError::InvalidBoolean(value)),
        }
    }

    fn read_u16(&mut self) -> Result<u16, VirtualMessageCodecError> {
        let bytes = self.read_array::<2>()?;

        Ok(u16::from_le_bytes(bytes))
    }

    fn read_i64(&mut self) -> Result<i64, VirtualMessageCodecError> {
        let bytes = self.read_array::<8>()?;

        Ok(i64::from_le_bytes(bytes))
    }

    fn read_f64(&mut self) -> Result<f64, VirtualMessageCodecError> {
        let bytes = self.read_array::<8>()?;

        Ok(f64::from_le_bytes(bytes))
    }

    fn read_string(&mut self) -> Result<String, VirtualMessageCodecError> {
        let length = usize::from(self.read_u16()?);

        let bytes = self.read_bytes(length)?;

        let value =
            std::str::from_utf8(bytes).map_err(|_| VirtualMessageCodecError::InvalidUtf8)?;

        Ok(value.to_owned())
    }

    fn read_array<const LENGTH: usize>(
        &mut self,
    ) -> Result<[u8; LENGTH], VirtualMessageCodecError> {
        let bytes = self.read_bytes(LENGTH)?;

        let mut array = [0_u8; LENGTH];
        array.copy_from_slice(bytes);

        Ok(array)
    }
}

#[derive(Debug)]
pub enum VirtualMessageCodecError {
    Frame(VirtualFrameError),
    Schema(VirtualInstrumentSchemaError),

    UnexpectedEnd,
    TrailingBytes(usize),
    InvalidUtf8,
    InvalidBoolean(u8),
    NonFiniteNumber,

    StringTooLong(usize),

    CollectionTooLong {
        collection: &'static str,
        length: usize,
    },

    DuplicateInstrumentId(VirtualInstrumentId),

    UnknownParameterAccess(u8),
    UnknownParameterValueType(u8),
    UnknownRangeType(u8),
    UnknownValueType(u8),
}

impl std::fmt::Display for VirtualMessageCodecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Frame(error) => error.fmt(formatter),

            Self::Schema(error) => error.fmt(formatter),

            Self::UnexpectedEnd => formatter.write_str(
                "Unexpected end of virtual \
                     instrument payload",
            ),

            Self::TrailingBytes(count) => {
                write!(
                    formatter,
                    "Virtual instrument payload has \
                     {count} trailing bytes",
                )
            }

            Self::InvalidUtf8 => formatter.write_str(
                "Virtual instrument string is not \
                     valid UTF-8",
            ),

            Self::InvalidBoolean(value) => {
                write!(
                    formatter,
                    "Invalid virtual instrument \
                     boolean value: {value}",
                )
            }

            Self::NonFiniteNumber => formatter.write_str(
                "Virtual instrument number must be \
                     finite",
            ),

            Self::StringTooLong(length) => {
                write!(
                    formatter,
                    "Virtual instrument string is too \
                     long: {length} bytes",
                )
            }

            Self::CollectionTooLong { collection, length } => {
                write!(
                    formatter,
                    "Too many virtual instrument \
                     {collection}: {length}",
                )
            }

            Self::DuplicateInstrumentId(id) => {
                write!(
                    formatter,
                    "Duplicate virtual instrument ID \
                     {id}",
                )
            }

            Self::UnknownParameterAccess(value) => {
                write!(
                    formatter,
                    "Unknown virtual parameter access: \
                     {value}",
                )
            }

            Self::UnknownParameterValueType(value) => {
                write!(
                    formatter,
                    "Unknown virtual parameter type: \
                     {value}",
                )
            }

            Self::UnknownRangeType(value) => {
                write!(
                    formatter,
                    "Unknown virtual parameter range \
                     type: {value}",
                )
            }

            Self::UnknownValueType(value) => {
                write!(
                    formatter,
                    "Unknown virtual instrument value \
                     type: {value}",
                )
            }
        }
    }
}

impl std::error::Error for VirtualMessageCodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Frame(error) => Some(error),
            Self::Schema(error) => Some(error),

            _ => None,
        }
    }
}

impl From<VirtualFrameError> for VirtualMessageCodecError {
    fn from(error: VirtualFrameError) -> Self {
        Self::Frame(error)
    }
}

impl From<VirtualInstrumentSchemaError> for VirtualMessageCodecError {
    fn from(error: VirtualInstrumentSchemaError) -> Self {
        Self::Schema(error)
    }
}

#[cfg(test)]
mod tests {
    use super::{VALUE_BOOLEAN, VirtualInstrumentMessage, VirtualMessageCodecError};

    use crate::{
        instrument::{
            InstrumentValue, ParameterAccess, ParameterRange, ParameterValueType,
            virtual_instrument::{
                VirtualInstrumentDescriptor, VirtualInstrumentId, VirtualParameterDescriptor,
                VirtualParameterId,
            },
        },
        protocol::virtual_instrument::{MessageKind, VirtualInstrumentFrame},
    };

    fn instrument_descriptor(id: u16) -> VirtualInstrumentDescriptor {
        let value = VirtualParameterDescriptor::new(
            VirtualParameterId::new(1),
            "value",
            "Signal value",
            ParameterAccess::ReadOnly,
            ParameterValueType::Number,
        )
        .with_range(ParameterRange::Number {
            minimum: -100.0,
            maximum: 100.0,
        })
        .with_unit("V")
        .with_series(true);

        let amplitude = VirtualParameterDescriptor::new(
            VirtualParameterId::new(2),
            "amplitude",
            "Amplitude",
            ParameterAccess::ReadWrite,
            ParameterValueType::Number,
        );

        VirtualInstrumentDescriptor::new(
            VirtualInstrumentId::new(id),
            format!("Generator {id}"),
            vec![value, amplitude],
        )
        .unwrap()
    }

    fn round_trip(message: VirtualInstrumentMessage) {
        let frame = message.encode_frame().unwrap();

        let decoded = VirtualInstrumentMessage::decode_frame(&frame).unwrap();

        assert_eq!(decoded, message);
    }

    #[test]
    fn round_trips_describe_request() {
        round_trip(VirtualInstrumentMessage::DescribeRequest);
    }

    #[test]
    fn round_trips_instrument_catalog() {
        round_trip(VirtualInstrumentMessage::DescribeResponse {
            instruments: vec![instrument_descriptor(1), instrument_descriptor(2)],
        });
    }

    #[test]
    fn round_trips_read_request() {
        round_trip(VirtualInstrumentMessage::ReadRequest {
            instrument: VirtualInstrumentId::new(7),
            parameter: VirtualParameterId::new(3),
        });
    }

    #[test]
    fn round_trips_values() {
        for value in [
            InstrumentValue::Boolean(true),
            InstrumentValue::Integer(-42),
            InstrumentValue::Number(12.5),
        ] {
            round_trip(VirtualInstrumentMessage::ReadResponse { value });

            round_trip(VirtualInstrumentMessage::WriteResponse { value });
        }
    }

    #[test]
    fn round_trips_write_request() {
        round_trip(VirtualInstrumentMessage::WriteRequest {
            instrument: VirtualInstrumentId::new(2),
            parameter: VirtualParameterId::new(4),
            value: InstrumentValue::Number(125.5),
        });
    }

    #[test]
    fn round_trips_error_response() {
        round_trip(VirtualInstrumentMessage::ErrorResponse {
            code: 17,
            message: "parameter is read-only".to_owned(),
        });
    }

    #[test]
    fn rejects_duplicate_instrument_ids() {
        let message = VirtualInstrumentMessage::DescribeResponse {
            instruments: vec![instrument_descriptor(1), instrument_descriptor(1)],
        };

        let result = message.encode_frame();

        assert!(matches!(
            result,
            Err(
                VirtualMessageCodecError::
                    DuplicateInstrumentId(id)
            ) if id == VirtualInstrumentId::new(1),
        ));
    }

    #[test]
    fn rejects_trailing_payload_bytes() {
        let frame = VirtualInstrumentFrame::new(MessageKind::DescribeRequest, vec![0]).unwrap();

        let result = VirtualInstrumentMessage::decode_frame(&frame);

        assert!(matches!(
            result,
            Err(VirtualMessageCodecError::TrailingBytes(1)),
        ));
    }

    #[test]
    fn rejects_invalid_boolean_value() {
        let frame =
            VirtualInstrumentFrame::new(MessageKind::ReadResponse, vec![VALUE_BOOLEAN, 2]).unwrap();

        let result = VirtualInstrumentMessage::decode_frame(&frame);

        assert!(matches!(
            result,
            Err(VirtualMessageCodecError::InvalidBoolean(2)),
        ));
    }
}
