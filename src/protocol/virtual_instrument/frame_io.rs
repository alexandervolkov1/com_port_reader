use std::io::{Read, Write};

use super::{
    CRC_LENGTH, HEADER_LENGTH, VirtualFrameError, VirtualInstrumentFrame, decode_frame_header,
};

pub fn read_frame<R>(reader: &mut R) -> Result<VirtualInstrumentFrame, VirtualFrameIoError>
where
    R: Read + ?Sized,
{
    let mut header = [0_u8; HEADER_LENGTH];

    reader.read_exact(&mut header)?;

    let (_, payload_length) = decode_frame_header(&header)?;

    let remaining_length = payload_length + CRC_LENGTH;

    let mut encoded = Vec::with_capacity(HEADER_LENGTH + remaining_length);

    encoded.extend_from_slice(&header);

    let payload_start = encoded.len();

    encoded.resize(HEADER_LENGTH + remaining_length, 0);

    reader.read_exact(&mut encoded[payload_start..])?;

    VirtualInstrumentFrame::decode(&encoded).map_err(VirtualFrameIoError::from)
}

pub fn write_frame<W>(
    writer: &mut W,
    frame: &VirtualInstrumentFrame,
) -> Result<(), VirtualFrameIoError>
where
    W: Write + ?Sized,
{
    let encoded = frame.encode();

    writer.write_all(&encoded)?;
    writer.flush()?;

    Ok(())
}

#[derive(Debug)]
pub enum VirtualFrameIoError {
    Io(std::io::Error),
    Frame(VirtualFrameError),
}

impl std::fmt::Display for VirtualFrameIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => {
                write!(
                    formatter,
                    "Virtual instrument I/O failed: \
                     {error}",
                )
            }

            Self::Frame(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for VirtualFrameIoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Frame(error) => Some(error),
        }
    }
}

impl From<std::io::Error> for VirtualFrameIoError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<VirtualFrameError> for VirtualFrameIoError {
    fn from(error: VirtualFrameError) -> Self {
        Self::Frame(error)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{VirtualFrameIoError, read_frame, write_frame};

    use crate::protocol::virtual_instrument::{MessageKind, VirtualInstrumentFrame};

    #[test]
    fn writes_encoded_frame() {
        let frame = VirtualInstrumentFrame::new(MessageKind::ReadRequest, vec![1, 2, 3]).unwrap();

        let expected = frame.encode();

        let mut output = Vec::new();

        write_frame(&mut output, &frame).unwrap();

        assert_eq!(output, expected);
    }

    #[test]
    fn reads_encoded_frame() {
        let expected =
            VirtualInstrumentFrame::new(MessageKind::WriteRequest, vec![4, 5, 6]).unwrap();

        let mut input = Cursor::new(expected.encode());

        let actual = read_frame(&mut input).unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn reads_consecutive_frames() {
        let first = VirtualInstrumentFrame::new(MessageKind::DescribeRequest, Vec::new()).unwrap();

        let second =
            VirtualInstrumentFrame::new(MessageKind::ReadRequest, vec![1, 0, 2, 0]).unwrap();

        let mut bytes = first.encode();

        bytes.extend_from_slice(&second.encode());

        let mut input = Cursor::new(bytes);

        assert_eq!(read_frame(&mut input).unwrap(), first,);

        assert_eq!(read_frame(&mut input).unwrap(), second,);
    }

    #[test]
    fn reports_truncated_frame() {
        let frame = VirtualInstrumentFrame::new(MessageKind::ReadResponse, vec![1, 1]).unwrap();

        let mut encoded = frame.encode();

        encoded.pop();

        let mut input = Cursor::new(encoded);

        let result = read_frame(&mut input);

        assert!(matches!(
            result,
            Err(VirtualFrameIoError::Io(error))
                if error.kind()
                    == std::io::ErrorKind::
                        UnexpectedEof,
        ));
    }

    #[test]
    fn rejects_invalid_header_before_body() {
        let mut encoded = VirtualInstrumentFrame::new(MessageKind::DescribeRequest, Vec::new())
            .unwrap()
            .encode();

        encoded[0] = b'X';

        let mut input = Cursor::new(encoded);

        let result = read_frame(&mut input);

        assert!(matches!(result, Err(VirtualFrameIoError::Frame(_)),));
    }
}
