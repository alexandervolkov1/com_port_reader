const READ_COMMAND: u8 = 0x00;

const READ_REQUEST_LENGTH: usize = 5;
const MIN_READ_RESPONSE_LENGTH: usize = 7;
const MAX_FRAME_LENGTH: usize = 38;

const TYPE_MASK: u8 = 0x0F;
const READABLE_MASK: u8 = 0x40;
const WRITABLE_MASK: u8 = 0x80;

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

#[cfg(test)]
mod tests {
    use super::{ReadRegisterRequest, ReadResponseError, RegisterValue, calculate_crc};

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

    fn with_crc(bytes: &[u8]) -> Vec<u8> {
        let mut frame = bytes.to_vec();

        frame.push(calculate_crc(bytes));

        frame
    }
}
