use std::io::{Read, Write};

use crate::{
    instrument::{
        InstrumentValue,
        virtual_instrument::{
            VirtualInstrumentDescriptor, VirtualInstrumentId, VirtualParameterId,
        },
    },
    serial_connection::SerialConnection,
};

use super::{
    MessageKind, VirtualFrameIoError, VirtualInstrumentMessage, VirtualMessageCodecError,
    read_frame, write_frame,
};

pub trait VirtualInstrumentTransport: Read + Write {
    fn clear_input(&mut self) -> std::io::Result<()>;
}

impl VirtualInstrumentTransport for SerialConnection {
    fn clear_input(&mut self) -> std::io::Result<()> {
        SerialConnection::clear_input(self).map_err(std::io::Error::other)
    }
}

pub struct VirtualInstrumentClient<'a, T>
where
    T: VirtualInstrumentTransport + ?Sized,
{
    transport: &'a mut T,
}

impl<'a, T> VirtualInstrumentClient<'a, T>
where
    T: VirtualInstrumentTransport + ?Sized,
{
    pub fn new(transport: &'a mut T) -> Self {
        Self { transport }
    }

    pub fn describe(
        &mut self,
    ) -> Result<Vec<VirtualInstrumentDescriptor>, VirtualInstrumentClientError> {
        let response = self.exchange(&VirtualInstrumentMessage::DescribeRequest)?;

        match response {
            VirtualInstrumentMessage::DescribeResponse { instruments } => Ok(instruments),

            response => Err(unexpected_response(
                MessageKind::DescribeResponse,
                response.kind(),
            )),
        }
    }

    pub fn read(
        &mut self,
        instrument: VirtualInstrumentId,
        parameter: VirtualParameterId,
    ) -> Result<InstrumentValue, VirtualInstrumentClientError> {
        let response = self.exchange(&VirtualInstrumentMessage::ReadRequest {
            instrument,
            parameter,
        })?;

        match response {
            VirtualInstrumentMessage::ReadResponse { value } => Ok(value),

            response => Err(unexpected_response(
                MessageKind::ReadResponse,
                response.kind(),
            )),
        }
    }

    pub fn write(
        &mut self,
        instrument: VirtualInstrumentId,
        parameter: VirtualParameterId,
        value: InstrumentValue,
    ) -> Result<InstrumentValue, VirtualInstrumentClientError> {
        let response = self.exchange(&VirtualInstrumentMessage::WriteRequest {
            instrument,
            parameter,
            value,
        })?;

        match response {
            VirtualInstrumentMessage::WriteResponse { value } => Ok(value),

            response => Err(unexpected_response(
                MessageKind::WriteResponse,
                response.kind(),
            )),
        }
    }

    fn exchange(
        &mut self,
        request: &VirtualInstrumentMessage,
    ) -> Result<VirtualInstrumentMessage, VirtualInstrumentClientError> {
        self.transport
            .clear_input()
            .map_err(VirtualInstrumentClientError::ClearInput)?;

        let request_frame = request.encode_frame()?;

        write_frame(self.transport, &request_frame)?;

        let response_frame = read_frame(self.transport)?;

        let response = VirtualInstrumentMessage::decode_frame(&response_frame)?;

        match response {
            VirtualInstrumentMessage::ErrorResponse { code, message } => {
                Err(VirtualInstrumentClientError::Device { code, message })
            }

            response => Ok(response),
        }
    }
}

fn unexpected_response(expected: MessageKind, actual: MessageKind) -> VirtualInstrumentClientError {
    VirtualInstrumentClientError::UnexpectedResponse { expected, actual }
}

#[derive(Debug)]
pub enum VirtualInstrumentClientError {
    ClearInput(std::io::Error),
    FrameIo(VirtualFrameIoError),
    Message(VirtualMessageCodecError),

    Device {
        code: u16,
        message: String,
    },

    UnexpectedResponse {
        expected: MessageKind,
        actual: MessageKind,
    },
}

impl std::fmt::Display for VirtualInstrumentClientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClearInput(error) => {
                write!(
                    formatter,
                    "Failed to clear virtual \
                     instrument input: {error}",
                )
            }

            Self::FrameIo(error) => error.fmt(formatter),

            Self::Message(error) => error.fmt(formatter),

            Self::Device { code, message } => {
                write!(
                    formatter,
                    "Virtual instrument error \
                     {code}: {message}",
                )
            }

            Self::UnexpectedResponse { expected, actual } => {
                write!(
                    formatter,
                    "Unexpected virtual instrument \
                     response: expected {expected:?}, \
                     received {actual:?}",
                )
            }
        }
    }
}

impl std::error::Error for VirtualInstrumentClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ClearInput(error) => Some(error),
            Self::FrameIo(error) => Some(error),
            Self::Message(error) => Some(error),

            Self::Device { .. } | Self::UnexpectedResponse { .. } => None,
        }
    }
}

impl From<VirtualFrameIoError> for VirtualInstrumentClientError {
    fn from(error: VirtualFrameIoError) -> Self {
        Self::FrameIo(error)
    }
}

impl From<VirtualMessageCodecError> for VirtualInstrumentClientError {
    fn from(error: VirtualMessageCodecError) -> Self {
        Self::Message(error)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{
        VirtualInstrumentClient, VirtualInstrumentClientError, VirtualInstrumentTransport,
    };

    use crate::{
        instrument::{
            InstrumentValue,
            virtual_instrument::{VirtualInstrumentId, VirtualParameterId},
        },
        protocol::virtual_instrument::{MessageKind, VirtualInstrumentMessage, read_frame},
    };

    struct TestTransport {
        input: Cursor<Vec<u8>>,
        output: Vec<u8>,
        clear_count: usize,
    }

    impl TestTransport {
        fn with_response(response: VirtualInstrumentMessage) -> Self {
            let frame = response.encode_frame().unwrap();

            Self {
                input: Cursor::new(frame.encode()),
                output: Vec::new(),
                clear_count: 0,
            }
        }
    }

    impl std::io::Read for TestTransport {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            self.input.read(buffer)
        }
    }

    impl std::io::Write for TestTransport {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.output.extend_from_slice(buffer);

            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl VirtualInstrumentTransport for TestTransport {
        fn clear_input(&mut self) -> std::io::Result<()> {
            self.clear_count += 1;
            Ok(())
        }
    }

    fn written_message(transport: &TestTransport) -> VirtualInstrumentMessage {
        let mut input = Cursor::new(transport.output.clone());

        let frame = read_frame(&mut input).unwrap();

        VirtualInstrumentMessage::decode_frame(&frame).unwrap()
    }

    #[test]
    fn requests_instrument_catalog() {
        let response = VirtualInstrumentMessage::DescribeResponse {
            instruments: Vec::new(),
        };

        let mut transport = TestTransport::with_response(response);

        let instruments = {
            let mut client = VirtualInstrumentClient::new(&mut transport);

            client.describe().unwrap()
        };

        assert!(instruments.is_empty());

        assert_eq!(
            written_message(&transport),
            VirtualInstrumentMessage::DescribeRequest,
        );

        assert_eq!(transport.clear_count, 1);
    }

    #[test]
    fn reads_parameter() {
        let response = VirtualInstrumentMessage::ReadResponse {
            value: InstrumentValue::Number(42.5),
        };

        let mut transport = TestTransport::with_response(response);

        let value = {
            let mut client = VirtualInstrumentClient::new(&mut transport);

            client
                .read(VirtualInstrumentId::new(2), VirtualParameterId::new(7))
                .unwrap()
        };

        assert_eq!(value, InstrumentValue::Number(42.5),);

        assert_eq!(
            written_message(&transport),
            VirtualInstrumentMessage::ReadRequest {
                instrument: VirtualInstrumentId::new(2),
                parameter: VirtualParameterId::new(7),
            },
        );
    }

    #[test]
    fn writes_parameter() {
        let response = VirtualInstrumentMessage::WriteResponse {
            value: InstrumentValue::Number(25.0),
        };

        let mut transport = TestTransport::with_response(response);

        let actual_value = {
            let mut client = VirtualInstrumentClient::new(&mut transport);

            client
                .write(
                    VirtualInstrumentId::new(3),
                    VirtualParameterId::new(4),
                    InstrumentValue::Number(25.0),
                )
                .unwrap()
        };

        assert_eq!(actual_value, InstrumentValue::Number(25.0),);

        assert_eq!(
            written_message(&transport),
            VirtualInstrumentMessage::WriteRequest {
                instrument: VirtualInstrumentId::new(3),
                parameter: VirtualParameterId::new(4),
                value: InstrumentValue::Number(25.0,),
            },
        );
    }

    #[test]
    fn reports_device_error() {
        let response = VirtualInstrumentMessage::ErrorResponse {
            code: 4,
            message: "unknown parameter".to_owned(),
        };

        let mut transport = TestTransport::with_response(response);

        let mut client = VirtualInstrumentClient::new(&mut transport);

        let result = client.read(VirtualInstrumentId::new(1), VirtualParameterId::new(99));

        assert!(matches!(
            result,
            Err(
                VirtualInstrumentClientError::
                    Device {
                        code: 4,
                        ref message,
                    }
            ) if message == "unknown parameter",
        ));
    }

    #[test]
    fn rejects_unexpected_response() {
        let response = VirtualInstrumentMessage::WriteResponse {
            value: InstrumentValue::Integer(1),
        };

        let mut transport = TestTransport::with_response(response);

        let mut client = VirtualInstrumentClient::new(&mut transport);

        let result = client.read(VirtualInstrumentId::new(1), VirtualParameterId::new(1));

        assert!(matches!(
            result,
            Err(VirtualInstrumentClientError::UnexpectedResponse {
                expected: MessageKind::ReadResponse,
                actual: MessageKind::WriteResponse,
            }),
        ));
    }
}
