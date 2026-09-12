use std::sync::{Arc, Mutex};

use crate::{
    acquisition::{AcquisitionError, AcquisitionSource},
    data::{Sample, SeriesMetadata, SeriesSource},
    instrument::{InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest},
    protocol::virtual_instrument::{
        VirtualInstrumentClient, VirtualInstrumentClientError, VirtualInstrumentTransport,
    },
    utils::current_time_f64,
};

/// Acquisition source for a virtual-instrument transport that is local to the
/// process. The transport itself is injected so this source does not know
/// whether it is backed by memory, a test double, or another local adapter.
pub struct LocalVirtualInstrumentSource<T>
where
    T: VirtualInstrumentTransport + Send,
{
    transport: Arc<Mutex<Option<T>>>,
}

impl<T> LocalVirtualInstrumentSource<T>
where
    T: VirtualInstrumentTransport + Send,
{
    #[cfg(test)]
    pub fn new(transport: T) -> Self {
        Self::with_session(Arc::new(Mutex::new(Some(transport))))
    }

    pub fn with_session(transport: Arc<Mutex<Option<T>>>) -> Self {
        Self { transport }
    }

    fn exchange<R>(
        &self,
        operation: impl FnOnce(
            &mut VirtualInstrumentClient<'_, T>,
        ) -> Result<R, VirtualInstrumentClientError>,
    ) -> Result<Option<R>, AcquisitionError> {
        // Hold the session for the entire request/response exchange: restart
        // must never replace an endpoint between writing and reading a frame.
        let mut session = self.transport.lock().unwrap_or_else(|e| e.into_inner());
        let transport = session
            .as_mut()
            .ok_or_else(|| AcquisitionError::from("Local emulator is stopped"))?;
        let result = operation(&mut VirtualInstrumentClient::new(transport));
        let recoverable_timeout = transport.supports_timeout_recovery()
            && matches!(&result,
                Err(VirtualInstrumentClientError::ClearInput(error)) |
                Err(VirtualInstrumentClientError::FrameIo(crate::protocol::virtual_instrument::VirtualFrameIoError::Io(error)))
                    if error.kind() == std::io::ErrorKind::TimedOut
            );
        match result {
            Ok(value) => Ok(Some(value)),
            Err(error) => {
                if !recoverable_timeout
                    && !matches!(error, VirtualInstrumentClientError::Device { .. })
                {
                    // Frames have no request IDs. After a timeout a late reply
                    // cannot safely be associated with another request.
                    session.take();
                    return Err(AcquisitionError::from(format!(
                        "{error}; local emulator transport closed, restart the emulator"
                    )));
                }
                Err(AcquisitionError::from(error.to_string()))
            }
        }
    }

    fn read_value(
        &mut self,
        request: InstrumentReadRequest,
    ) -> Result<Option<InstrumentValue>, AcquisitionError> {
        let InstrumentReadRequest::VirtualInstrument {
            instrument,
            parameter,
        } = request
        else {
            return Ok(None);
        };

        self.exchange(|client| client.read(instrument, parameter))
    }
}

impl<T> AcquisitionSource for LocalVirtualInstrumentSource<T>
where
    T: VirtualInstrumentTransport + Send,
{
    fn sample_series(
        &mut self,
        series: &SeriesMetadata,
    ) -> Result<Option<Sample>, AcquisitionError> {
        let SeriesSource::Instrument(request) = &series.source else {
            return Ok(None);
        };
        let Some(value) = self.read_value(*request)? else {
            return Ok(None);
        };

        Ok(Some(Sample::new(current_time_f64(), value.as_f64())))
    }

    fn describe_virtual_instruments(
        &mut self,
    ) -> Result<
        Option<Vec<crate::instrument::virtual_instrument::VirtualInstrumentDescriptor>>,
        AcquisitionError,
    > {
        self.exchange(|client| client.describe())
    }

    fn read_instrument(
        &mut self,
        request: InstrumentReadRequest,
    ) -> Result<Option<InstrumentValue>, AcquisitionError> {
        self.read_value(request)
    }

    fn write_instrument(
        &mut self,
        request: InstrumentWriteRequest,
    ) -> Result<Option<InstrumentValue>, AcquisitionError> {
        let InstrumentWriteRequest::VirtualInstrument {
            instrument,
            parameter,
            value,
        } = request
        else {
            return Ok(None);
        };

        self.exchange(|client| client.write(instrument, parameter, value))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::io::{Read, Write};

    use super::{AcquisitionSource, LocalVirtualInstrumentSource};
    use crate::{
        connection::ConnectionId,
        data::{SeriesId, SeriesMetadata, SeriesPollingState},
        instrument::{InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest},
        protocol::virtual_instrument::{VirtualInstrumentMessage, VirtualInstrumentTransport},
    };

    struct TestTransport {
        input: VecDeque<u8>,
        output: Vec<u8>,
    }

    impl TestTransport {
        fn with_responses(responses: &[VirtualInstrumentMessage]) -> Self {
            let mut input = VecDeque::new();
            for response in responses {
                input.extend(response.encode_frame().unwrap().encode());
            }
            Self {
                input,
                output: Vec::new(),
            }
        }
    }

    impl Read for TestTransport {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if self.input.is_empty() {
                return Ok(0);
            }
            let length = buffer.len().min(self.input.len());
            for byte in &mut buffer[..length] {
                *byte = self.input.pop_front().expect("length was checked");
            }
            Ok(length)
        }
    }

    impl Write for TestTransport {
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
            Ok(())
        }
    }

    fn metadata(request: InstrumentReadRequest) -> SeriesMetadata {
        SeriesMetadata {
            id: SeriesId::new(1),
            connection_id: ConnectionId::PRIMARY,
            name: "temperature".to_owned(),
            source: crate::data::SeriesSource::Instrument(request),
            sampling_interval: None,
            presentation: crate::data::series::SeriesPresentation::default(),
            polling_state: SeriesPollingState::Enabled,
        }
    }

    #[test]
    fn reads_virtual_instrument_and_samples_series() {
        let request = InstrumentReadRequest::virtual_instrument(
            crate::instrument::virtual_instrument::VirtualInstrumentId::new(1),
            crate::instrument::virtual_instrument::VirtualParameterId::new(2),
        );
        let mut source = LocalVirtualInstrumentSource::new(TestTransport::with_responses(&[
            VirtualInstrumentMessage::ReadResponse {
                value: InstrumentValue::Number(42.5),
            },
            VirtualInstrumentMessage::ReadResponse {
                value: InstrumentValue::Number(43.0),
            },
        ]));

        assert_eq!(
            source.read_instrument(request).unwrap(),
            Some(InstrumentValue::Number(42.5))
        );
        let sample = source.sample_series(&metadata(request)).unwrap().unwrap();
        assert_eq!(sample.value, 43.0);
        assert!(sample.timestamp.is_finite());
    }

    #[test]
    fn writes_virtual_instrument_and_describes_catalog() {
        let descriptor = crate::instrument::virtual_instrument::VirtualInstrumentDescriptor::new(
            crate::instrument::virtual_instrument::VirtualInstrumentId::new(1),
            "Furnace",
            vec![
                crate::instrument::virtual_instrument::VirtualParameterDescriptor::new(
                    crate::instrument::virtual_instrument::VirtualParameterId::new(1),
                    "power",
                    "Power",
                    crate::instrument::ParameterAccess::ReadWrite,
                    crate::instrument::ParameterValueType::Number,
                ),
            ],
        )
        .unwrap();
        let request = InstrumentWriteRequest::virtual_instrument(
            crate::instrument::virtual_instrument::VirtualInstrumentId::new(1),
            crate::instrument::virtual_instrument::VirtualParameterId::new(1),
            InstrumentValue::Number(50.0),
        );
        let mut source = LocalVirtualInstrumentSource::new(TestTransport::with_responses(&[
            VirtualInstrumentMessage::DescribeResponse {
                instruments: vec![descriptor],
            },
            VirtualInstrumentMessage::WriteResponse {
                value: InstrumentValue::Number(49.5),
            },
        ]));

        assert_eq!(
            source
                .describe_virtual_instruments()
                .unwrap()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            source.write_instrument(request).unwrap(),
            Some(InstrumentValue::Number(49.5))
        );
    }

    #[test]
    fn ignores_non_virtual_operations() {
        let mut source = LocalVirtualInstrumentSource::new(TestTransport::with_responses(&[]));
        let series = SeriesMetadata {
            id: SeriesId::new(1),
            connection_id: ConnectionId::PRIMARY,
            name: "serial".to_owned(),
            source: crate::data::SeriesSource::SerialCommand {
                command: "read".to_owned(),
            },
            sampling_interval: None,
            presentation: crate::data::series::SeriesPresentation::default(),
            polling_state: SeriesPollingState::Enabled,
        };

        assert_eq!(source.sample_series(&series).unwrap(), None);
        assert_eq!(source.request_text("read").unwrap(), None);
    }

    #[test]
    fn timeout_closes_session_and_rejects_late_response() {
        use crate::protocol::virtual_instrument::MemoryEndpoint;
        let (client, mut server) =
            MemoryEndpoint::with_timeout(std::time::Duration::from_millis(1));
        let mut source = LocalVirtualInstrumentSource::new(client);
        let error = source.describe_virtual_instruments().unwrap_err();
        assert!(error.to_string().contains("restart the emulator"));
        assert_eq!(
            server.write(b"late response").unwrap_err().kind(),
            std::io::ErrorKind::BrokenPipe
        );
        assert_eq!(
            source
                .describe_virtual_instruments()
                .unwrap_err()
                .to_string(),
            "Local emulator is stopped"
        );
    }

    #[test]
    fn model_error_keeps_session_available_for_retry() {
        let mut source = LocalVirtualInstrumentSource::new(TestTransport::with_responses(&[
            VirtualInstrumentMessage::ErrorResponse {
                code: 1,
                message: "model failure".into(),
            },
            VirtualInstrumentMessage::DescribeResponse {
                instruments: Vec::new(),
            },
        ]));
        assert!(
            source
                .describe_virtual_instruments()
                .unwrap_err()
                .to_string()
                .contains("model failure")
        );
        assert!(source.describe_virtual_instruments().unwrap().is_some());
    }

    #[test]
    fn memory_timeout_recovers_without_replaying_write_or_accepting_stale_read() {
        use crate::instrument::virtual_instrument::{VirtualInstrumentId, VirtualParameterId};
        use crate::protocol::virtual_instrument::{
            MemoryClientTransport, MemoryEndpoint, read_frame, write_frame,
        };
        for initial_write in [false, true] {
            let (client, mut server) =
                MemoryEndpoint::with_timeout(std::time::Duration::from_millis(100));
            let mut source = LocalVirtualInstrumentSource::new(MemoryClientTransport::new(client));
            let instrument = VirtualInstrumentId::new(1);
            let parameter = VirtualParameterId::new(1);
            let error = if initial_write {
                source
                    .write_instrument(InstrumentWriteRequest::virtual_instrument(
                        instrument,
                        parameter,
                        InstrumentValue::Number(75.0),
                    ))
                    .unwrap_err()
            } else {
                source
                    .read_instrument(InstrumentReadRequest::virtual_instrument(
                        instrument, parameter,
                    ))
                    .unwrap_err()
            };
            assert!(error.to_string().contains("timed out"));
            assert!(!error.to_string().contains("restart"));
            let first =
                VirtualInstrumentMessage::decode_frame(&read_frame(&mut server).unwrap()).unwrap();
            // Model state is modified once, by the original write only.
            let mut model_state = 42.0;
            let late = if initial_write {
                let VirtualInstrumentMessage::WriteRequest { value, .. } = first else {
                    panic!("expected write")
                };
                model_state = value.as_f64();
                VirtualInstrumentMessage::WriteResponse { value }
            } else {
                assert!(matches!(
                    first,
                    VirtualInstrumentMessage::ReadRequest { .. }
                ));
                VirtualInstrumentMessage::ReadResponse {
                    value: InstrumentValue::Number(-1.0),
                }
            }
            .encode_frame()
            .unwrap()
            .encode();
            server.write_all(&late[..3]).unwrap();
            let next =
                InstrumentReadRequest::virtual_instrument(instrument, VirtualParameterId::new(2));
            assert!(
                source
                    .read_instrument(next)
                    .unwrap_err()
                    .to_string()
                    .contains("timed out")
            );
            // No new command is sent while the old response is incomplete.
            assert_eq!(
                server.read(&mut [0; 1]).unwrap_err().kind(),
                std::io::ErrorKind::TimedOut
            );
            server.write_all(&late[3..]).unwrap();
            let responder = std::thread::spawn(move || {
                let request =
                    VirtualInstrumentMessage::decode_frame(&read_frame(&mut server).unwrap())
                        .unwrap();
                assert!(
                    matches!(request, VirtualInstrumentMessage::ReadRequest { parameter, .. } if parameter == VirtualParameterId::new(2))
                );
                write_frame(
                    &mut server,
                    &VirtualInstrumentMessage::ReadResponse {
                        value: InstrumentValue::Number(model_state),
                    }
                    .encode_frame()
                    .unwrap(),
                )
                .unwrap();
            });
            assert_eq!(
                source.read_instrument(next).unwrap(),
                Some(InstrumentValue::Number(model_state))
            );
            responder.join().unwrap();
        }
    }

    #[test]
    fn combined_routes_virtual_locally_and_metakon_to_serial_even_when_stopped() {
        use crate::{
            acquisition::{CombinedSource, SerialCommandSource},
            instrument::{
                metakon_5x3::{Metakon5x3, Metakon5x3Register, Metakon5x3Write},
                virtual_instrument::{VirtualInstrumentId, VirtualParameterId},
            },
            serial_connection::SerialConfigStore,
        };
        let local = LocalVirtualInstrumentSource::new(TestTransport::with_responses(&[
            VirtualInstrumentMessage::DescribeResponse {
                instruments: Vec::new(),
            },
            VirtualInstrumentMessage::ReadResponse {
                value: InstrumentValue::Number(42.0),
            },
            VirtualInstrumentMessage::WriteResponse {
                value: InstrumentValue::Number(50.0),
            },
            VirtualInstrumentMessage::ReadResponse {
                value: InstrumentValue::Number(43.0),
            },
        ]));
        let session = local.transport.clone();
        let mut combined = CombinedSource::new(vec![
            Box::new(local),
            Box::new(SerialCommandSource::new(SerialConfigStore::new())),
        ]);
        let read = InstrumentReadRequest::virtual_instrument(
            VirtualInstrumentId::new(1),
            VirtualParameterId::new(1),
        );
        let write = InstrumentWriteRequest::virtual_instrument(
            VirtualInstrumentId::new(1),
            VirtualParameterId::new(1),
            InstrumentValue::Number(50.0),
        );
        assert!(
            combined
                .describe_virtual_instruments()
                .unwrap()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            combined.read_instrument(read).unwrap(),
            Some(InstrumentValue::Number(42.0))
        );
        assert_eq!(
            combined.write_instrument(write).unwrap(),
            Some(InstrumentValue::Number(50.0))
        );
        assert_eq!(
            combined
                .sample_series(&metadata(read))
                .unwrap()
                .unwrap()
                .value,
            43.0
        );
        for stopped in [false, true] {
            if stopped {
                session.lock().unwrap().take();
                for error in [
                    combined.describe_virtual_instruments().unwrap_err(),
                    combined.read_instrument(read).unwrap_err(),
                    combined.write_instrument(write).unwrap_err(),
                    combined.sample_series(&metadata(read)).unwrap_err(),
                ] {
                    assert_eq!(error.to_string(), "Local emulator is stopped");
                }
            }
            let metakon_read = InstrumentReadRequest::metakon_5x3(
                Metakon5x3::new(1, 0),
                Metakon5x3Register::Measurement,
                1.0,
            );
            let metakon_write = InstrumentWriteRequest::metakon_5x3(
                Metakon5x3::new(1, 0),
                Metakon5x3Write::Setpoint(35),
                1.0,
            )
            .unwrap();
            for error in [
                combined.request_text("status").unwrap_err(),
                combined.read_instrument(metakon_read).unwrap_err(),
                combined.write_instrument(metakon_write).unwrap_err(),
                combined.sample_series(&metadata(metakon_read)).unwrap_err(),
            ] {
                assert!(
                    error.to_string().contains("COM port is not selected"),
                    "{error}"
                );
            }
        }
    }
}
