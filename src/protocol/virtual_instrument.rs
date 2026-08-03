mod client;
mod frame_io;
mod message;

pub use client::{
    VirtualInstrumentClient, VirtualInstrumentClientError, VirtualInstrumentTransport,
};

pub use frame_io::{VirtualFrameIoError, read_frame, write_frame};

pub use message::{VirtualInstrumentMessage, VirtualMessageCodecError};

pub const MAGIC: [u8; 2] = *b"VI";
pub const VERSION: u8 = 1;

pub const HEADER_LENGTH: usize = 6;
pub const CRC_LENGTH: usize = 2;
pub const MAX_PAYLOAD_LENGTH: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    DescribeRequest,
    DescribeResponse,

    ReadRequest,
    ReadResponse,

    WriteRequest,
    WriteResponse,

    ErrorResponse,
}

impl MessageKind {
    pub const fn code(self) -> u8 {
        match self {
            Self::DescribeRequest => 0x01,
            Self::ReadRequest => 0x02,
            Self::WriteRequest => 0x03,

            Self::DescribeResponse => 0x81,
            Self::ReadResponse => 0x82,
            Self::WriteResponse => 0x83,

            Self::ErrorResponse => 0xFF,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            0x01 => Some(Self::DescribeRequest),
            0x02 => Some(Self::ReadRequest),
            0x03 => Some(Self::WriteRequest),

            0x81 => Some(Self::DescribeResponse),
            0x82 => Some(Self::ReadResponse),
            0x83 => Some(Self::WriteResponse),

            0xFF => Some(Self::ErrorResponse),

            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualInstrumentFrame {
    kind: MessageKind,
    payload: Vec<u8>,
}

impl VirtualInstrumentFrame {
    pub fn new(kind: MessageKind, payload: Vec<u8>) -> Result<Self, VirtualFrameError> {
        if payload.len() > MAX_PAYLOAD_LENGTH {
            return Err(VirtualFrameError::PayloadTooLong {
                length: payload.len(),
                maximum: MAX_PAYLOAD_LENGTH,
            });
        }

        Ok(Self { kind, payload })
    }

    pub const fn kind(&self) -> MessageKind {
        self.kind
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn encode(&self) -> Vec<u8> {
        let payload_length = u16::try_from(self.payload.len()).expect("validated payload length");

        let mut frame = Vec::with_capacity(HEADER_LENGTH + self.payload.len() + CRC_LENGTH);

        frame.extend_from_slice(&MAGIC);
        frame.push(VERSION);
        frame.push(self.kind.code());

        frame.extend_from_slice(&payload_length.to_le_bytes());

        frame.extend_from_slice(&self.payload);

        let crc = calculate_crc16(&frame);

        frame.extend_from_slice(&crc.to_le_bytes());

        frame
    }

    pub fn decode(frame: &[u8]) -> Result<Self, VirtualFrameError> {
        let minimum_length = HEADER_LENGTH + CRC_LENGTH;

        if frame.len() < minimum_length {
            return Err(VirtualFrameError::FrameTooShort {
                length: frame.len(),
                minimum: minimum_length,
            });
        }

        let (kind, payload_length) = decode_frame_header(&frame[..HEADER_LENGTH])?;

        let expected_frame_length = HEADER_LENGTH + payload_length + CRC_LENGTH;

        if frame.len() != expected_frame_length {
            return Err(VirtualFrameError::FrameLengthMismatch {
                expected: expected_frame_length,
                actual: frame.len(),
            });
        }

        let payload_end = HEADER_LENGTH + payload_length;

        let expected_crc = calculate_crc16(&frame[..payload_end]);

        let actual_crc = u16::from_le_bytes([frame[payload_end], frame[payload_end + 1]]);

        if actual_crc != expected_crc {
            return Err(VirtualFrameError::CrcMismatch {
                expected: expected_crc,
                actual: actual_crc,
            });
        }

        Ok(Self {
            kind,
            payload: frame[HEADER_LENGTH..payload_end].to_vec(),
        })
    }
}

pub(crate) fn decode_frame_header(
    header: &[u8],
) -> Result<(MessageKind, usize), VirtualFrameError> {
    if header.len() < HEADER_LENGTH {
        return Err(VirtualFrameError::FrameTooShort {
            length: header.len(),
            minimum: HEADER_LENGTH,
        });
    }

    let actual_magic = [header[0], header[1]];

    if actual_magic != MAGIC {
        return Err(VirtualFrameError::InvalidMagic {
            actual: actual_magic,
        });
    }

    let actual_version = header[2];

    if actual_version != VERSION {
        return Err(VirtualFrameError::UnsupportedVersion {
            actual: actual_version,
        });
    }

    let kind = MessageKind::from_code(header[3])
        .ok_or(VirtualFrameError::UnknownMessageKind(header[3]))?;

    let payload_length = usize::from(u16::from_le_bytes([header[4], header[5]]));

    if payload_length > MAX_PAYLOAD_LENGTH {
        return Err(VirtualFrameError::PayloadTooLong {
            length: payload_length,
            maximum: MAX_PAYLOAD_LENGTH,
        });
    }

    Ok((kind, payload_length))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VirtualFrameError {
    FrameTooShort { length: usize, minimum: usize },

    InvalidMagic { actual: [u8; 2] },

    UnsupportedVersion { actual: u8 },

    UnknownMessageKind(u8),

    PayloadTooLong { length: usize, maximum: usize },

    FrameLengthMismatch { expected: usize, actual: usize },

    CrcMismatch { expected: u16, actual: u16 },
}

impl std::fmt::Display for VirtualFrameError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FrameTooShort { length, minimum } => {
                write!(
                    formatter,
                    "Virtual instrument frame is too \
                     short: {length} bytes, minimum \
                     is {minimum}",
                )
            }

            Self::InvalidMagic { actual } => {
                write!(
                    formatter,
                    "Invalid virtual instrument frame \
                     magic: {:02X} {:02X}",
                    actual[0], actual[1],
                )
            }

            Self::UnsupportedVersion { actual } => {
                write!(
                    formatter,
                    "Unsupported virtual instrument \
                     protocol version: {actual}",
                )
            }

            Self::UnknownMessageKind(kind) => {
                write!(
                    formatter,
                    "Unknown virtual instrument \
                     message kind: 0x{kind:02X}",
                )
            }

            Self::PayloadTooLong { length, maximum } => {
                write!(
                    formatter,
                    "Virtual instrument payload is too \
                     long: {length} bytes, maximum is \
                     {maximum}",
                )
            }

            Self::FrameLengthMismatch { expected, actual } => {
                write!(
                    formatter,
                    "Virtual instrument frame length \
                     mismatch: expected {expected} \
                     bytes, received {actual}",
                )
            }

            Self::CrcMismatch { expected, actual } => {
                write!(
                    formatter,
                    "Virtual instrument CRC mismatch: \
                     expected 0x{expected:04X}, \
                     received 0x{actual:04X}",
                )
            }
        }
    }
}

impl std::error::Error for VirtualFrameError {}

/// CRC-16/MODBUS. The result is written to the frame
/// in little-endian byte order.
pub fn calculate_crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0xFFFF_u16;

    for &byte in bytes {
        crc ^= u16::from(byte);

        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }

    crc
}

#[cfg(test)]
mod tests {
    use super::{
        HEADER_LENGTH, MAGIC, MAX_PAYLOAD_LENGTH, MessageKind, VERSION, VirtualFrameError,
        VirtualInstrumentFrame, calculate_crc16,
    };

    #[test]
    fn calculates_known_modbus_crc() {
        assert_eq!(calculate_crc16(b"123456789"), 0x4B37,);
    }

    #[test]
    fn round_trips_all_message_kinds() {
        let kinds = [
            MessageKind::DescribeRequest,
            MessageKind::DescribeResponse,
            MessageKind::ReadRequest,
            MessageKind::ReadResponse,
            MessageKind::WriteRequest,
            MessageKind::WriteResponse,
            MessageKind::ErrorResponse,
        ];

        for kind in kinds {
            let original = VirtualInstrumentFrame::new(kind, vec![1, 2, 3, 4]).unwrap();

            let encoded = original.encode();

            let decoded = VirtualInstrumentFrame::decode(&encoded).unwrap();

            assert_eq!(decoded, original);
        }
    }

    #[test]
    fn writes_binary_header() {
        let frame = VirtualInstrumentFrame::new(MessageKind::ReadRequest, vec![0xAA, 0xBB])
            .unwrap()
            .encode();

        assert_eq!(&frame[0..2], &MAGIC);
        assert_eq!(frame[2], VERSION);

        assert_eq!(frame[3], MessageKind::ReadRequest.code(),);

        assert_eq!(&frame[4..6], &2_u16.to_le_bytes(),);

        assert_eq!(&frame[HEADER_LENGTH..HEADER_LENGTH + 2], &[0xAA, 0xBB],);
    }

    #[test]
    fn rejects_short_frame() {
        let result = VirtualInstrumentFrame::decode(&[b'V', b'I', 1]);

        assert_eq!(
            result,
            Err(VirtualFrameError::FrameTooShort {
                length: 3,
                minimum: 8,
            },),
        );
    }

    #[test]
    fn rejects_invalid_magic() {
        let mut frame = VirtualInstrumentFrame::new(MessageKind::DescribeRequest, Vec::new())
            .unwrap()
            .encode();

        frame[0] = b'X';

        let result = VirtualInstrumentFrame::decode(&frame);

        assert_eq!(
            result,
            Err(VirtualFrameError::InvalidMagic {
                actual: [b'X', b'I'],
            },),
        );
    }

    #[test]
    fn rejects_unsupported_version() {
        let mut frame = VirtualInstrumentFrame::new(MessageKind::DescribeRequest, Vec::new())
            .unwrap()
            .encode();

        frame[2] = VERSION + 1;

        let result = VirtualInstrumentFrame::decode(&frame);

        assert_eq!(
            result,
            Err(VirtualFrameError::UnsupportedVersion {
                actual: VERSION + 1,
            },),
        );
    }

    #[test]
    fn rejects_unknown_message_kind() {
        let mut frame = VirtualInstrumentFrame::new(MessageKind::DescribeRequest, Vec::new())
            .unwrap()
            .encode();

        frame[3] = 0x7F;

        let result = VirtualInstrumentFrame::decode(&frame);

        assert_eq!(result, Err(VirtualFrameError::UnknownMessageKind(0x7F),),);
    }

    #[test]
    fn rejects_incorrect_frame_length() {
        let mut frame = VirtualInstrumentFrame::new(MessageKind::ReadRequest, vec![1, 2])
            .unwrap()
            .encode();

        frame[4..6].copy_from_slice(&3_u16.to_le_bytes());

        let result = VirtualInstrumentFrame::decode(&frame);

        assert_eq!(
            result,
            Err(VirtualFrameError::FrameLengthMismatch {
                expected: 11,
                actual: 10,
            },),
        );
    }

    #[test]
    fn rejects_corrupted_payload() {
        let mut frame = VirtualInstrumentFrame::new(MessageKind::WriteRequest, vec![1, 2, 3])
            .unwrap()
            .encode();

        frame[HEADER_LENGTH] ^= 0xFF;

        let result = VirtualInstrumentFrame::decode(&frame);

        assert!(matches!(result, Err(VirtualFrameError::CrcMismatch { .. }),));
    }

    #[test]
    fn rejects_oversized_payload() {
        let result = VirtualInstrumentFrame::new(
            MessageKind::DescribeResponse,
            vec![0; MAX_PAYLOAD_LENGTH + 1],
        );

        assert_eq!(
            result,
            Err(VirtualFrameError::PayloadTooLong {
                length: MAX_PAYLOAD_LENGTH + 1,
                maximum: MAX_PAYLOAD_LENGTH,
            },),
        );
    }
}
