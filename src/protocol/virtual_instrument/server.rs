use std::time::Duration;

use crate::instrument::{
    InstrumentValue, ParameterRange, ParameterValueType,
    virtual_instrument::{
        VirtualInstrumentDescriptor, VirtualInstrumentId, VirtualParameterDescriptor,
        VirtualParameterId,
    },
};

use super::VirtualInstrumentMessage;

pub const ERROR_INVALID_REQUEST: u16 = 1;
pub const ERROR_UNKNOWN_INSTRUMENT: u16 = 2;
pub const ERROR_UNKNOWN_PARAMETER: u16 = 3;
pub const ERROR_ACCESS_DENIED: u16 = 4;
pub const ERROR_TYPE_MISMATCH: u16 = 5;
pub const ERROR_INVALID_VALUE: u16 = 6;
pub const ERROR_OUT_OF_RANGE: u16 = 7;
pub const ERROR_MODEL_FAILURE: u16 = 100;

pub trait VirtualInstrumentModel {
    fn instruments(&self) -> &[VirtualInstrumentDescriptor];

    fn read(
        &mut self,
        instrument: VirtualInstrumentId,
        parameter: VirtualParameterId,
        elapsed: Duration,
    ) -> Result<InstrumentValue, VirtualInstrumentModelError>;

    fn write(
        &mut self,
        instrument: VirtualInstrumentId,
        parameter: VirtualParameterId,
        value: InstrumentValue,
        elapsed: Duration,
    ) -> Result<InstrumentValue, VirtualInstrumentModelError>;
}

pub struct VirtualInstrumentServer<M> {
    model: M,
}

impl<M> VirtualInstrumentServer<M>
where
    M: VirtualInstrumentModel,
{
    pub fn new(model: M) -> Self {
        Self { model }
    }

    pub fn handle(
        &mut self,
        request: VirtualInstrumentMessage,
        elapsed: Duration,
    ) -> VirtualInstrumentMessage {
        match request {
            VirtualInstrumentMessage::DescribeRequest => {
                VirtualInstrumentMessage::DescribeResponse {
                    instruments: self.model.instruments().to_vec(),
                }
            }

            VirtualInstrumentMessage::ReadRequest {
                instrument,
                parameter,
            } => match self.read(instrument, parameter, elapsed) {
                Ok(value) => VirtualInstrumentMessage::ReadResponse { value },

                Err(error) => error.into_response(),
            },

            VirtualInstrumentMessage::WriteRequest {
                instrument,
                parameter,
                value,
            } => match self.write(instrument, parameter, value, elapsed) {
                Ok(value) => VirtualInstrumentMessage::WriteResponse { value },

                Err(error) => error.into_response(),
            },

            response => ServerError::new(
                ERROR_INVALID_REQUEST,
                format!(
                    "Expected virtual instrument \
                     request, received {:?}",
                    response.kind(),
                ),
            )
            .into_response(),
        }
    }

    fn read(
        &mut self,
        instrument: VirtualInstrumentId,
        parameter: VirtualParameterId,
        elapsed: Duration,
    ) -> Result<InstrumentValue, ServerError> {
        let descriptor = self.parameter_descriptor(instrument, parameter)?;

        if !descriptor.access().readable() {
            return Err(ServerError::new(
                ERROR_ACCESS_DENIED,
                format!(
                    "Virtual parameter '{}' is not \
                     readable",
                    descriptor.key(),
                ),
            ));
        }

        let value = self
            .model
            .read(instrument, parameter, elapsed)
            .map_err(model_failure)?;

        validate_model_value(&descriptor, value)?;

        Ok(value)
    }

    fn write(
        &mut self,
        instrument: VirtualInstrumentId,
        parameter: VirtualParameterId,
        value: InstrumentValue,
        elapsed: Duration,
    ) -> Result<InstrumentValue, ServerError> {
        let descriptor = self.parameter_descriptor(instrument, parameter)?;

        if !descriptor.access().writable() {
            return Err(ServerError::new(
                ERROR_ACCESS_DENIED,
                format!(
                    "Virtual parameter '{}' is not \
                     writable",
                    descriptor.key(),
                ),
            ));
        }

        validate_input_value(&descriptor, value)?;

        let actual_value = self
            .model
            .write(instrument, parameter, value, elapsed)
            .map_err(model_failure)?;

        validate_model_value(&descriptor, actual_value)?;

        Ok(actual_value)
    }

    fn parameter_descriptor(
        &self,
        instrument_id: VirtualInstrumentId,
        parameter_id: VirtualParameterId,
    ) -> Result<VirtualParameterDescriptor, ServerError> {
        let instrument = self
            .model
            .instruments()
            .iter()
            .find(|instrument| instrument.id() == instrument_id)
            .ok_or_else(|| {
                ServerError::new(
                    ERROR_UNKNOWN_INSTRUMENT,
                    format!(
                        "Unknown virtual instrument \
                         ID {instrument_id}",
                    ),
                )
            })?;

        instrument
            .parameter_by_id(parameter_id)
            .cloned()
            .ok_or_else(|| {
                ServerError::new(
                    ERROR_UNKNOWN_PARAMETER,
                    format!(
                        "Unknown parameter ID \
                         {parameter_id} for virtual \
                         instrument {instrument_id}",
                    ),
                )
            })
    }
}

fn validate_input_value(
    descriptor: &VirtualParameterDescriptor,
    value: InstrumentValue,
) -> Result<(), ServerError> {
    validate_value(descriptor, value).map_err(|error| {
        ServerError::new(
            error.code(),
            format!(
                "Invalid value for virtual \
                     parameter '{}': {error}",
                descriptor.key(),
            ),
        )
    })
}

fn validate_model_value(
    descriptor: &VirtualParameterDescriptor,
    value: InstrumentValue,
) -> Result<(), ServerError> {
    validate_value(descriptor, value).map_err(|error| {
        ServerError::new(
            ERROR_MODEL_FAILURE,
            format!(
                "Virtual model returned invalid \
                     value for parameter '{}': \
                     {error}",
                descriptor.key(),
            ),
        )
    })
}

fn validate_value(
    descriptor: &VirtualParameterDescriptor,
    value: InstrumentValue,
) -> Result<(), ValueValidationError> {
    let actual_type = value_type(value);

    if actual_type != descriptor.value_type() {
        return Err(ValueValidationError::TypeMismatch {
            expected: descriptor.value_type(),
            actual: actual_type,
        });
    }

    match value {
        InstrumentValue::Number(value) if !value.is_finite() => {
            return Err(ValueValidationError::NonFiniteNumber);
        }

        _ => {}
    }

    match (value, descriptor.range()) {
        (InstrumentValue::Integer(value), Some(ParameterRange::Integer { minimum, maximum }))
            if value < minimum || value > maximum =>
        {
            Err(ValueValidationError::OutOfRange)
        }

        (InstrumentValue::Number(value), Some(ParameterRange::Number { minimum, maximum }))
            if value < minimum || value > maximum =>
        {
            Err(ValueValidationError::OutOfRange)
        }

        _ => Ok(()),
    }
}

fn value_type(value: InstrumentValue) -> ParameterValueType {
    match value {
        InstrumentValue::Boolean(_) => ParameterValueType::Boolean,

        InstrumentValue::Integer(_) => ParameterValueType::Integer,

        InstrumentValue::Number(_) => ParameterValueType::Number,
    }
}

fn model_failure(error: VirtualInstrumentModelError) -> ServerError {
    ServerError::new(ERROR_MODEL_FAILURE, error.to_string())
}

#[derive(Debug)]
enum ValueValidationError {
    TypeMismatch {
        expected: ParameterValueType,
        actual: ParameterValueType,
    },

    NonFiniteNumber,
    OutOfRange,
}

impl ValueValidationError {
    const fn code(&self) -> u16 {
        match self {
            Self::TypeMismatch { .. } => ERROR_TYPE_MISMATCH,

            Self::NonFiniteNumber => ERROR_INVALID_VALUE,

            Self::OutOfRange => ERROR_OUT_OF_RANGE,
        }
    }
}

impl std::fmt::Display for ValueValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TypeMismatch { expected, actual } => {
                write!(
                    formatter,
                    "expected {}, received {}",
                    expected.as_str(),
                    actual.as_str(),
                )
            }

            Self::NonFiniteNumber => formatter.write_str("number must be finite"),

            Self::OutOfRange => formatter.write_str(
                "value is outside the declared \
                     range",
            ),
        }
    }
}

struct ServerError {
    code: u16,
    message: String,
}

impl ServerError {
    fn new(code: u16, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn into_response(self) -> VirtualInstrumentMessage {
        VirtualInstrumentMessage::ErrorResponse {
            code: self.code,
            message: self.message,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualInstrumentModelError {
    message: String,
}

impl std::fmt::Display for VirtualInstrumentModelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for VirtualInstrumentModelError {}

impl From<String> for VirtualInstrumentModelError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<&str> for VirtualInstrumentModelError {
    fn from(message: &str) -> Self {
        Self {
            message: message.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        ERROR_ACCESS_DENIED, ERROR_MODEL_FAILURE, ERROR_OUT_OF_RANGE, ERROR_TYPE_MISMATCH,
        ERROR_UNKNOWN_INSTRUMENT, VirtualInstrumentModel, VirtualInstrumentModelError,
        VirtualInstrumentServer,
    };

    use crate::{
        instrument::{
            InstrumentValue, ParameterAccess, ParameterRange, ParameterValueType,
            virtual_instrument::{
                VirtualInstrumentDescriptor, VirtualInstrumentId, VirtualParameterDescriptor,
                VirtualParameterId,
            },
        },
        protocol::virtual_instrument::VirtualInstrumentMessage,
    };

    const INSTRUMENT_ID: VirtualInstrumentId = VirtualInstrumentId::new(1);

    const VALUE_ID: VirtualParameterId = VirtualParameterId::new(1);

    const AMPLITUDE_ID: VirtualParameterId = VirtualParameterId::new(2);

    const RESET_ID: VirtualParameterId = VirtualParameterId::new(3);

    struct TestModel {
        instruments: Vec<VirtualInstrumentDescriptor>,
        amplitude: f64,
        fail_read: bool,
    }

    impl TestModel {
        fn new() -> Self {
            let value = VirtualParameterDescriptor::new(
                VALUE_ID,
                "value",
                "Value",
                ParameterAccess::ReadOnly,
                ParameterValueType::Number,
            )
            .with_series(true);

            let amplitude = VirtualParameterDescriptor::new(
                AMPLITUDE_ID,
                "amplitude",
                "Amplitude",
                ParameterAccess::ReadWrite,
                ParameterValueType::Number,
            )
            .with_range(ParameterRange::Number {
                minimum: 0.0,
                maximum: 100.0,
            });

            let reset = VirtualParameterDescriptor::new(
                RESET_ID,
                "reset",
                "Reset",
                ParameterAccess::WriteOnly,
                ParameterValueType::Boolean,
            );

            let instrument = VirtualInstrumentDescriptor::new(
                INSTRUMENT_ID,
                "Generator",
                vec![value, amplitude, reset],
            )
            .unwrap();

            Self {
                instruments: vec![instrument],
                amplitude: 10.0,
                fail_read: false,
            }
        }
    }

    impl VirtualInstrumentModel for TestModel {
        fn instruments(&self) -> &[VirtualInstrumentDescriptor] {
            &self.instruments
        }

        fn read(
            &mut self,
            _instrument: VirtualInstrumentId,
            parameter: VirtualParameterId,
            _elapsed: Duration,
        ) -> Result<InstrumentValue, VirtualInstrumentModelError> {
            if self.fail_read {
                return Err(VirtualInstrumentModelError::from("simulated failure"));
            }

            match parameter {
                VALUE_ID => Ok(InstrumentValue::Number(self.amplitude * 2.0)),

                AMPLITUDE_ID => Ok(InstrumentValue::Number(self.amplitude)),

                _ => Err(VirtualInstrumentModelError::from("unsupported read")),
            }
        }

        fn write(
            &mut self,
            _instrument: VirtualInstrumentId,
            parameter: VirtualParameterId,
            value: InstrumentValue,
            _elapsed: Duration,
        ) -> Result<InstrumentValue, VirtualInstrumentModelError> {
            match (parameter, value) {
                (AMPLITUDE_ID, InstrumentValue::Number(value)) => {
                    self.amplitude = value;

                    Ok(InstrumentValue::Number(value))
                }

                (RESET_ID, InstrumentValue::Boolean(value)) => {
                    if value {
                        self.amplitude = 0.0;
                    }

                    Ok(InstrumentValue::Boolean(value))
                }

                _ => Err(VirtualInstrumentModelError::from("unsupported write")),
            }
        }
    }

    fn error_code(response: VirtualInstrumentMessage) -> u16 {
        let VirtualInstrumentMessage::ErrorResponse { code, .. } = response else {
            panic!("expected error response");
        };

        code
    }

    #[test]
    fn describes_model() {
        let model = TestModel::new();

        let mut server = VirtualInstrumentServer::new(model);

        let response = server.handle(VirtualInstrumentMessage::DescribeRequest, Duration::ZERO);

        let VirtualInstrumentMessage::DescribeResponse { instruments } = response else {
            panic!("expected describe response");
        };

        assert_eq!(instruments.len(), 1);

        assert_eq!(instruments[0].name(), "Generator",);
    }

    #[test]
    fn reads_parameter() {
        let model = TestModel::new();

        let mut server = VirtualInstrumentServer::new(model);

        let response = server.handle(
            VirtualInstrumentMessage::ReadRequest {
                instrument: INSTRUMENT_ID,
                parameter: VALUE_ID,
            },
            Duration::ZERO,
        );

        assert_eq!(
            response,
            VirtualInstrumentMessage::ReadResponse {
                value: InstrumentValue::Number(20.0,),
            },
        );
    }

    #[test]
    fn writes_parameter() {
        let model = TestModel::new();

        let mut server = VirtualInstrumentServer::new(model);

        let response = server.handle(
            VirtualInstrumentMessage::WriteRequest {
                instrument: INSTRUMENT_ID,
                parameter: AMPLITUDE_ID,
                value: InstrumentValue::Number(25.0),
            },
            Duration::ZERO,
        );

        assert_eq!(
            response,
            VirtualInstrumentMessage::WriteResponse {
                value: InstrumentValue::Number(25.0,),
            },
        );

        let response = server.handle(
            VirtualInstrumentMessage::ReadRequest {
                instrument: INSTRUMENT_ID,
                parameter: AMPLITUDE_ID,
            },
            Duration::ZERO,
        );

        assert_eq!(
            response,
            VirtualInstrumentMessage::ReadResponse {
                value: InstrumentValue::Number(25.0,),
            },
        );
    }

    #[test]
    fn rejects_unknown_instrument() {
        let mut server = VirtualInstrumentServer::new(TestModel::new());

        let response = server.handle(
            VirtualInstrumentMessage::ReadRequest {
                instrument: VirtualInstrumentId::new(99),
                parameter: VALUE_ID,
            },
            Duration::ZERO,
        );

        assert_eq!(error_code(response), ERROR_UNKNOWN_INSTRUMENT,);
    }

    #[test]
    fn rejects_reading_write_only_parameter() {
        let mut server = VirtualInstrumentServer::new(TestModel::new());

        let response = server.handle(
            VirtualInstrumentMessage::ReadRequest {
                instrument: INSTRUMENT_ID,
                parameter: RESET_ID,
            },
            Duration::ZERO,
        );

        assert_eq!(error_code(response), ERROR_ACCESS_DENIED,);
    }

    #[test]
    fn rejects_writing_read_only_parameter() {
        let mut server = VirtualInstrumentServer::new(TestModel::new());

        let response = server.handle(
            VirtualInstrumentMessage::WriteRequest {
                instrument: INSTRUMENT_ID,
                parameter: VALUE_ID,
                value: InstrumentValue::Number(1.0),
            },
            Duration::ZERO,
        );

        assert_eq!(error_code(response), ERROR_ACCESS_DENIED,);
    }

    #[test]
    fn rejects_wrong_value_type() {
        let mut server = VirtualInstrumentServer::new(TestModel::new());

        let response = server.handle(
            VirtualInstrumentMessage::WriteRequest {
                instrument: INSTRUMENT_ID,
                parameter: AMPLITUDE_ID,
                value: InstrumentValue::Integer(25),
            },
            Duration::ZERO,
        );

        assert_eq!(error_code(response), ERROR_TYPE_MISMATCH,);
    }

    #[test]
    fn rejects_value_outside_range() {
        let mut server = VirtualInstrumentServer::new(TestModel::new());

        let response = server.handle(
            VirtualInstrumentMessage::WriteRequest {
                instrument: INSTRUMENT_ID,
                parameter: AMPLITUDE_ID,
                value: InstrumentValue::Number(125.0),
            },
            Duration::ZERO,
        );

        assert_eq!(error_code(response), ERROR_OUT_OF_RANGE,);
    }

    #[test]
    fn reports_model_failure() {
        let mut model = TestModel::new();
        model.fail_read = true;

        let mut server = VirtualInstrumentServer::new(model);

        let response = server.handle(
            VirtualInstrumentMessage::ReadRequest {
                instrument: INSTRUMENT_ID,
                parameter: VALUE_ID,
            },
            Duration::ZERO,
        );

        assert_eq!(error_code(response), ERROR_MODEL_FAILURE,);
    }
}
