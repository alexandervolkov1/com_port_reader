use crate::serial_connection::{SerialConnection, SerialConnectionError};

const READ_COMMAND: u8 = 0x00;

const READ_REQUEST_LENGTH: usize = 5;
const MIN_READ_RESPONSE_LENGTH: usize = 7;
const MAX_FRAME_LENGTH: usize = 38;

const TYPE_MASK: u8 = 0x0F;
const READABLE_MASK: u8 = 0x40;
const WRITABLE_MASK: u8 = 0x80;

const READ_ATTEMPTS: usize = 3;
const READ_RESPONSE_OVERHEAD: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadRegisterRequest {
    device: u8,
    channel: u8,
    register: u8,
}

impl ReadRegisterRequest {
    pub const fn new(device: u8, channel: u8, register: u8) -> Self {
        Self {
            device,
            channel,
            register,
        }
    }

    pub fn encode(self) -> [u8; READ_REQUEST_LENGTH] {
        let mut frame = [self.device, self.channel, self.register, READ_COMMAND, 0];

        let crc = calculate_crc(&frame[..READ_REQUEST_LENGTH - 1]);

        frame[READ_REQUEST_LENGTH - 1] = crc;

        frame
    }

    pub fn decode_response(self, frame: &[u8]) -> Result<ReadRegisterResponse, ReadResponseError> {
        if frame.len() < MIN_READ_RESPONSE_LENGTH {
            return Err(ReadResponseError::FrameTooShort(frame.len()));
        }

        if frame.len() > MAX_FRAME_LENGTH {
            return Err(ReadResponseError::FrameTooLong(frame.len()));
        }

        let payload_end = frame.len() - 1;

        let expected_crc = calculate_crc(&frame[..payload_end]);
        let actual_crc = frame[payload_end];

        if actual_crc != expected_crc {
            return Err(ReadResponseError::CrcMismatch {
                expected: expected_crc,
                actual: actual_crc,
            });
        }

        if frame[0] != self.device {
            return Err(ReadResponseError::UnexpectedDevice {
                expected: self.device,
                actual: frame[0],
            });
        }

        if frame[1] != self.channel {
            return Err(ReadResponseError::UnexpectedChannel {
                expected: self.channel,
                actual: frame[1],
            });
        }

        if frame[2] != self.register {
            return Err(ReadResponseError::UnexpectedRegister {
                expected: self.register,
                actual: frame[2],
            });
        }

        if frame[3] != READ_COMMAND {
            return Err(ReadResponseError::UnexpectedCommand {
                expected: READ_COMMAND,
                actual: frame[3],
            });
        }

        let type_byte = frame[4];
        let type_code = type_byte & TYPE_MASK;

        let value = parse_register_value(type_code, &frame[5..payload_end])?;

        Ok(ReadRegisterResponse {
            device: frame[0],
            channel: frame[1],
            register: frame[2],
            readable: type_byte & READABLE_MASK != 0,
            writable: type_byte & WRITABLE_MASK != 0,
            value,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterDataType {
    Bool,
    Ubyte,
    Byte,
    Uint,
    Int,
    Ulong,
    Long,
    Float,
    Double,
}

impl RegisterDataType {
    const fn data_length(self) -> usize {
        match self {
            Self::Bool | Self::Ubyte | Self::Byte => 1,

            Self::Uint | Self::Int => 2,

            Self::Ulong | Self::Long | Self::Float => 4,

            Self::Double => 8,
        }
    }

    const fn response_length(self) -> usize {
        READ_RESPONSE_OVERHEAD + self.data_length()
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Bool => "Bool",
            Self::Ubyte => "Ubyte",
            Self::Byte => "Byte",
            Self::Uint => "Uint",
            Self::Int => "Int",
            Self::Ulong => "Ulong",
            Self::Long => "Long",
            Self::Float => "Float",
            Self::Double => "Double",
        }
    }

    const fn matches_value(self, value: &RegisterValue) -> bool {
        matches!(
            (self, value),
            (Self::Bool, RegisterValue::Bool(_))
                | (Self::Ubyte, RegisterValue::Ubyte(_))
                | (Self::Byte, RegisterValue::Byte(_))
                | (Self::Uint, RegisterValue::Uint(_))
                | (Self::Int, RegisterValue::Int(_))
                | (Self::Ulong, RegisterValue::Ulong(_))
                | (Self::Long, RegisterValue::Long(_))
                | (Self::Float, RegisterValue::Float(_))
                | (Self::Double, RegisterValue::Double(_))
        )
    }
}

pub fn read_register(
    connection: &mut SerialConnection,
    request: ReadRegisterRequest,
    expected_type: RegisterDataType,
) -> Result<RegisterValue, ReadRegisterError> {
    let mut last_error = match read_register_once(connection, request, expected_type) {
        Ok(value) => return Ok(value),
        Err(error) => error,
    };

    for _ in 1..READ_ATTEMPTS {
        match read_register_once(connection, request, expected_type) {
            Ok(value) => return Ok(value),
            Err(error) => last_error = error,
        }
    }

    Err(ReadRegisterError {
        attempts: READ_ATTEMPTS,
        last_error,
    })
}

pub fn read_int_register(
    connection: &mut SerialConnection,
    request: ReadRegisterRequest,
) -> Result<i16, ReadIntRegisterError> {
    let value = read_register(connection, request, RegisterDataType::Int)?;

    let RegisterValue::Int(value) = value else {
        unreachable!("read_register returned a value with an unexpected type");
    };

    Ok(value)
}

fn read_register_once(
    connection: &mut SerialConnection,
    request: ReadRegisterRequest,
    expected_type: RegisterDataType,
) -> Result<RegisterValue, ReadRegisterAttemptError> {
    let request_bytes = request.encode();

    let mut response_bytes = vec![0_u8; expected_type.response_length()];

    connection.exchange_exact(&request_bytes, &mut response_bytes)?;

    let response = request.decode_response(&response_bytes)?;

    let value = response.into_value();

    if !expected_type.matches_value(&value) {
        return Err(ReadRegisterAttemptError::UnexpectedValueType {
            expected: expected_type,
            actual: register_value_type_name(&value),
        });
    }

    Ok(value)
}

fn register_value_type_name(value: &RegisterValue) -> &'static str {
    match value {
        RegisterValue::Bool(_) => "Bool",
        RegisterValue::Ubyte(_) => "Ubyte",
        RegisterValue::Byte(_) => "Byte",
        RegisterValue::Uint(_) => "Uint",
        RegisterValue::Int(_) => "Int",
        RegisterValue::Ulong(_) => "Ulong",
        RegisterValue::Long(_) => "Long",
        RegisterValue::Float(_) => "Float",
        RegisterValue::Double(_) => "Double",
        RegisterValue::Ascii(_) => "ASCIIZ",
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReadRegisterResponse {
    device: u8,
    channel: u8,
    register: u8,
    readable: bool,
    writable: bool,
    value: RegisterValue,
}

impl ReadRegisterResponse {
    pub const fn device(&self) -> u8 {
        self.device
    }

    pub const fn channel(&self) -> u8 {
        self.channel
    }

    pub const fn register(&self) -> u8 {
        self.register
    }

    pub const fn readable(&self) -> bool {
        self.readable
    }

    pub const fn writable(&self) -> bool {
        self.writable
    }

    pub const fn value(&self) -> &RegisterValue {
        &self.value
    }

    pub fn into_value(self) -> RegisterValue {
        self.value
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RegisterValue {
    Bool(bool),
    Ubyte(u8),
    Byte(i8),
    Uint(u16),
    Int(i16),
    Ulong(u32),
    Long(i32),
    Float(f32),
    Double(f64),
    Ascii(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadResponseError {
    FrameTooShort(usize),
    FrameTooLong(usize),

    CrcMismatch {
        expected: u8,
        actual: u8,
    },

    UnexpectedDevice {
        expected: u8,
        actual: u8,
    },

    UnexpectedChannel {
        expected: u8,
        actual: u8,
    },

    UnexpectedRegister {
        expected: u8,
        actual: u8,
    },

    UnexpectedCommand {
        expected: u8,
        actual: u8,
    },

    UnsupportedType(u8),

    InvalidDataLength {
        type_code: u8,
        expected: usize,
        actual: usize,
    },

    InvalidBool(u8),
    InvalidAscii,
}

impl std::fmt::Display for ReadResponseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FrameTooShort(length) => {
                write!(
                    formatter,
                    "Metakon response is too short: \
                     {length} bytes",
                )
            }

            Self::FrameTooLong(length) => {
                write!(
                    formatter,
                    "Metakon response is too long: \
                     {length} bytes",
                )
            }

            Self::CrcMismatch { expected, actual } => {
                write!(
                    formatter,
                    "Metakon CRC mismatch: expected \
                     {expected:02X}, received {actual:02X}",
                )
            }

            Self::UnexpectedDevice { expected, actual } => {
                write!(
                    formatter,
                    "Unexpected Metakon device address: \
                     expected {expected}, received {actual}",
                )
            }

            Self::UnexpectedChannel { expected, actual } => {
                write!(
                    formatter,
                    "Unexpected Metakon channel: \
                     expected {expected}, received {actual}",
                )
            }

            Self::UnexpectedRegister { expected, actual } => {
                write!(
                    formatter,
                    "Unexpected Metakon register: \
                     expected {expected:02X}, \
                     received {actual:02X}",
                )
            }

            Self::UnexpectedCommand { expected, actual } => {
                write!(
                    formatter,
                    "Unexpected Metakon command: \
                     expected {expected:02X}, \
                     received {actual:02X}",
                )
            }

            Self::UnsupportedType(type_code) => {
                write!(
                    formatter,
                    "Unsupported Metakon register type: \
                     {type_code}",
                )
            }

            Self::InvalidDataLength {
                type_code,
                expected,
                actual,
            } => {
                write!(
                    formatter,
                    "Invalid Metakon data length for type \
                     {type_code}: expected {expected}, \
                     received {actual}",
                )
            }

            Self::InvalidBool(value) => {
                write!(
                    formatter,
                    "Invalid Metakon boolean value: \
                     {value:02X}",
                )
            }

            Self::InvalidAscii => formatter.write_str("Invalid Metakon ASCIIZ value"),
        }
    }
}

impl std::error::Error for ReadResponseError {}

fn parse_register_value(type_code: u8, data: &[u8]) -> Result<RegisterValue, ReadResponseError> {
    match type_code {
        0 => {
            require_data_length(type_code, data, 1)?;

            match data[0] {
                0x00 => Ok(RegisterValue::Bool(false)),
                0xFF => Ok(RegisterValue::Bool(true)),

                value => Err(ReadResponseError::InvalidBool(value)),
            }
        }

        1 => {
            require_data_length(type_code, data, 1)?;

            Ok(RegisterValue::Ubyte(data[0]))
        }

        2 => {
            require_data_length(type_code, data, 1)?;

            Ok(RegisterValue::Byte(data[0] as i8))
        }

        3 => {
            require_data_length(type_code, data, 2)?;

            Ok(RegisterValue::Uint(u16::from_le_bytes([data[0], data[1]])))
        }

        4 => {
            require_data_length(type_code, data, 2)?;

            Ok(RegisterValue::Int(i16::from_le_bytes([data[0], data[1]])))
        }

        5 => {
            require_data_length(type_code, data, 4)?;

            Ok(RegisterValue::Ulong(u32::from_le_bytes([
                data[0], data[1], data[2], data[3],
            ])))
        }

        6 => {
            require_data_length(type_code, data, 4)?;

            Ok(RegisterValue::Long(i32::from_le_bytes([
                data[0], data[1], data[2], data[3],
            ])))
        }

        7 => {
            require_data_length(type_code, data, 4)?;

            Ok(RegisterValue::Float(f32::from_le_bytes([
                data[0], data[1], data[2], data[3],
            ])))
        }

        8 => {
            require_data_length(type_code, data, 8)?;

            Ok(RegisterValue::Double(f64::from_le_bytes([
                data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
            ])))
        }

        9 => parse_ascii(data),

        _ => Err(ReadResponseError::UnsupportedType(type_code)),
    }
}

fn parse_ascii(data: &[u8]) -> Result<RegisterValue, ReadResponseError> {
    if data.is_empty() || data.len() > 32 || data.last() != Some(&0) {
        return Err(ReadResponseError::InvalidAscii);
    }

    let text_bytes = &data[..data.len() - 1];

    if !text_bytes.is_ascii() || text_bytes.contains(&0) {
        return Err(ReadResponseError::InvalidAscii);
    }

    let text =
        String::from_utf8(text_bytes.to_vec()).map_err(|_| ReadResponseError::InvalidAscii)?;

    Ok(RegisterValue::Ascii(text))
}

fn require_data_length(
    type_code: u8,
    data: &[u8],
    expected: usize,
) -> Result<(), ReadResponseError> {
    if data.len() != expected {
        return Err(ReadResponseError::InvalidDataLength {
            type_code,
            expected,
            actual: data.len(),
        });
    }

    Ok(())
}

/// Calculates the one-byte CRC used by the Metakon protocol.
pub fn calculate_crc(bytes: &[u8]) -> u8 {
    let mut crc = 0xFF_u8;

    for &byte in bytes {
        let mut data = byte;

        for _ in 0..8 {
            let feedback = (data ^ crc) & 1;

            if feedback != 0 {
                crc ^= 0x18;
            }

            crc >>= 1;
            crc |= feedback << 7;

            data >>= 1;
        }
    }

    crc
}

pub type ReadIntRegisterError = ReadRegisterError;

#[derive(Debug)]
pub struct ReadRegisterError {
    attempts: usize,
    last_error: ReadRegisterAttemptError,
}

impl std::fmt::Display for ReadRegisterError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Metakon register read failed after {} attempts: {}",
            self.attempts, self.last_error,
        )
    }
}

impl std::error::Error for ReadRegisterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.last_error)
    }
}

#[derive(Debug)]
enum ReadRegisterAttemptError {
    Serial(SerialConnectionError),

    InvalidResponse(ReadResponseError),

    UnexpectedValueType {
        expected: RegisterDataType,
        actual: &'static str,
    },
}

impl std::fmt::Display for ReadRegisterAttemptError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serial(error) => error.fmt(formatter),

            Self::InvalidResponse(error) => error.fmt(formatter),

            Self::UnexpectedValueType { expected, actual } => {
                write!(
                    formatter,
                    "Expected Metakon {} response, got {actual}",
                    expected.name(),
                )
            }
        }
    }
}

impl std::error::Error for ReadRegisterAttemptError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Serial(error) => Some(error),

            Self::InvalidResponse(error) => Some(error),

            Self::UnexpectedValueType { .. } => None,
        }
    }
}

impl From<SerialConnectionError> for ReadRegisterAttemptError {
    fn from(error: SerialConnectionError) -> Self {
        Self::Serial(error)
    }
}

impl From<ReadResponseError> for ReadRegisterAttemptError {
    fn from(error: ReadResponseError) -> Self {
        Self::InvalidResponse(error)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ReadRegisterRequest, ReadResponseError, RegisterDataType, RegisterValue, calculate_crc,
    };

    #[test]
    fn matches_single_byte_reference_values() {
        assert_eq!(calculate_crc(&[0x00]), 0x35);
        assert_eq!(calculate_crc(&[0x01]), 0x6B);
        assert_eq!(calculate_crc(&[0xFF]), 0x00);
    }

    #[test]
    fn encodes_device_one_read_request() {
        let request = ReadRegisterRequest::new(0x01, 0x00, 0x01);

        assert_eq!(request.encode(), [0x01, 0x00, 0x01, 0x00, 0xA0],);
    }

    #[test]
    fn encodes_device_two_read_request() {
        let request = ReadRegisterRequest::new(0x02, 0x00, 0x01);

        assert_eq!(request.encode(), [0x02, 0x00, 0x01, 0x00, 0x28],);
    }

    #[test]
    fn parses_positive_int_response() {
        let request = ReadRegisterRequest::new(0x01, 0x00, 0x01);

        let response = request
            .decode_response(&[
                0x01, // DEV
                0x00, // CHA
                0x01, // REG
                0x00, // RD
                0x44, // readable Int
                0xD2, // DATA low
                0x04, // DATA high
                0xF1, // CRC
            ])
            .unwrap();

        assert_eq!(response.device(), 1);
        assert_eq!(response.channel(), 0);
        assert_eq!(response.register(), 1);
        assert!(response.readable());
        assert!(!response.writable());

        assert_eq!(response.value(), &RegisterValue::Int(1234),);
    }

    #[test]
    fn parses_negative_int_response() {
        let request = ReadRegisterRequest::new(0x02, 0x01, 0x01);

        let response = request
            .decode_response(&[
                0x02, // DEV
                0x01, // CHA
                0x01, // REG
                0x00, // RD
                0x44, // readable Int
                0x85, // DATA low
                0xFF, // DATA high
                0xCC, // CRC
            ])
            .unwrap();

        assert_eq!(response.value(), &RegisterValue::Int(-123),);
    }

    #[test]
    fn rejects_corrupted_crc() {
        let request = ReadRegisterRequest::new(0x01, 0x00, 0x01);

        let error = request
            .decode_response(&[0x01, 0x00, 0x01, 0x00, 0x44, 0xD2, 0x04, 0x00])
            .unwrap_err();

        assert!(matches!(error, ReadResponseError::CrcMismatch { .. },));
    }

    #[test]
    fn rejects_response_for_another_device() {
        let request = ReadRegisterRequest::new(0x01, 0x00, 0x01);

        let frame = with_crc(&[0x02, 0x00, 0x01, 0x00, 0x44, 0xD2, 0x04]);

        let error = request.decode_response(&frame).unwrap_err();

        assert_eq!(
            error,
            ReadResponseError::UnexpectedDevice {
                expected: 1,
                actual: 2,
            },
        );
    }

    #[test]
    fn rejects_invalid_int_length() {
        let request = ReadRegisterRequest::new(0x01, 0x00, 0x01);

        let frame = with_crc(&[0x01, 0x00, 0x01, 0x00, 0x44, 0xD2]);

        let error = request.decode_response(&frame).unwrap_err();

        assert_eq!(
            error,
            ReadResponseError::InvalidDataLength {
                type_code: 4,
                expected: 2,
                actual: 1,
            },
        );
    }

    #[test]
    fn calculates_fixed_response_lengths() {
        assert_eq!(RegisterDataType::Byte.response_length(), 7,);

        assert_eq!(RegisterDataType::Int.response_length(), 8,);

        assert_eq!(RegisterDataType::Double.response_length(), 14,);
    }

    #[test]
    fn parses_negative_byte_response() {
        let request = ReadRegisterRequest::new(0x0F, 0x00, 0x06);

        let frame = with_crc(&[
            0x0F, // DEV
            0x00, // CHA
            0x06, // output power register
            0x00, // read command
            0xC2, // readable + writable Byte
            0xE7, // -25 as i8
        ]);

        let response = request.decode_response(&frame).unwrap();

        assert!(response.readable());
        assert!(response.writable());

        assert_eq!(response.value(), &RegisterValue::Byte(-25),);
    }

    fn with_crc(bytes: &[u8]) -> Vec<u8> {
        let mut frame = bytes.to_vec();

        frame.push(calculate_crc(bytes));

        frame
    }
}
