use crate::{
    acquisition::{AcquisitionError, AcquisitionSource},
    data::{Sample, SeriesMetadata, SeriesSource},
    instrument::{InstrumentReadRequest, InstrumentValue, InstrumentWriteRequest},
    protocol::virtual_instrument::{VirtualInstrumentClient, VirtualInstrumentTransport},
    utils::current_time_f64,
};

/// Acquisition source for a virtual-instrument transport that is local to the
/// process. The transport itself is injected so this source does not know
/// whether it is backed by memory, a test double, or another local adapter.
#[allow(dead_code)] // Wired into worker construction in the local-emulator integration step.
pub struct LocalVirtualInstrumentSource<T>
where
    T: VirtualInstrumentTransport + Send,
{
    transport: T,
}

#[allow(dead_code)]
impl<T> LocalVirtualInstrumentSource<T>
where
    T: VirtualInstrumentTransport + Send,
{
    pub fn new(transport: T) -> Self {
        Self { transport }
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

        let mut client = VirtualInstrumentClient::new(&mut self.transport);
        client
            .read(instrument, parameter)
            .map(Some)
            .map_err(|error| AcquisitionError::from(error.to_string()))
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
        let mut client = VirtualInstrumentClient::new(&mut self.transport);
        client
            .describe()
            .map(Some)
            .map_err(|error| AcquisitionError::from(error.to_string()))
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

        let mut client = VirtualInstrumentClient::new(&mut self.transport);
        client
            .write(instrument, parameter, value)
            .map(Some)
            .map_err(|error| AcquisitionError::from(error.to_string()))
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
}
