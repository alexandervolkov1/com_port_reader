use std::{error::Error, fmt};

use super::{
    ControllerKind, ControllerParameterError,
    controller::{expect_number, unknown_parameter},
};
use crate::instrument::{
    InstrumentValue, ParameterAccess, ParameterDescriptor, ParameterRange, ParameterValueType,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OnOffOutput {
    value: f64,
    active: bool,
}

impl OnOffOutput {
    pub const fn value(self) -> f64 {
        self.value
    }

    pub const fn active(self) -> bool {
        self.active
    }
}

#[derive(Debug)]
/// Hysteretic two-state controller for outputs that are either fully on or fully off.
pub struct OnOffController {
    setpoint: f64,
    hysteresis: f64,
    output_off: f64,
    output_on: f64,
    active: bool,
}

impl OnOffController {
    pub fn new(
        setpoint: f64,
        hysteresis: f64,
        output_off: f64,
        output_on: f64,
    ) -> Result<Self, OnOffControllerError> {
        validate_configuration(setpoint, hysteresis, output_off, output_on)?;

        Ok(Self {
            setpoint,
            hysteresis,
            output_off,
            output_on,
            active: false,
        })
    }

    pub fn configure(
        &mut self,
        setpoint: f64,
        hysteresis: f64,
        output_off: f64,
        output_on: f64,
    ) -> Result<(), OnOffControllerError> {
        validate_configuration(setpoint, hysteresis, output_off, output_on)?;

        self.setpoint = setpoint;
        self.hysteresis = hysteresis;
        self.output_off = output_off;
        self.output_on = output_on;

        Ok(())
    }

    pub const fn setpoint(&self) -> f64 {
        self.setpoint
    }

    pub const fn hysteresis(&self) -> f64 {
        self.hysteresis
    }

    pub const fn output_off(&self) -> f64 {
        self.output_off
    }

    pub const fn output_on(&self) -> f64 {
        self.output_on
    }

    pub const fn active(&self) -> bool {
        self.active
    }

    pub fn update(
        &mut self,
        timestamp: f64,
        measurement: f64,
    ) -> Result<OnOffOutput, OnOffControllerError> {
        if !timestamp.is_finite() {
            return Err(OnOffControllerError::NonFiniteTimestamp);
        }

        if !measurement.is_finite() {
            return Err(OnOffControllerError::NonFiniteMeasurement);
        }

        let lower = self.setpoint - self.hysteresis;

        let upper = self.setpoint + self.hysteresis;

        if measurement < lower {
            self.active = true;
        } else if measurement > upper {
            self.active = false;
        }

        Ok(OnOffOutput {
            value: if self.active {
                self.output_on
            } else {
                self.output_off
            },
            active: self.active,
        })
    }

    pub fn reset(&mut self) {
        self.active = false;
    }

    pub fn resynchronize(&mut self) {
        // On/off control has no time-dependent
        // runtime state to resynchronize.
    }
}

fn validate_configuration(
    setpoint: f64,
    hysteresis: f64,
    output_off: f64,
    output_on: f64,
) -> Result<(), OnOffControllerError> {
    if !setpoint.is_finite() {
        return Err(OnOffControllerError::NonFiniteSetpoint);
    }

    if !hysteresis.is_finite() {
        return Err(OnOffControllerError::NonFiniteHysteresis);
    }

    if hysteresis < 0.0 {
        return Err(OnOffControllerError::NegativeHysteresis);
    }

    if !output_off.is_finite() {
        return Err(OnOffControllerError::NonFiniteOutputOff);
    }

    if !output_on.is_finite() {
        return Err(OnOffControllerError::NonFiniteOutputOn);
    }

    let lower = setpoint - hysteresis;
    let upper = setpoint + hysteresis;

    if !lower.is_finite() || !upper.is_finite() {
        return Err(OnOffControllerError::NonFiniteThreshold);
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OnOffControllerError {
    NonFiniteTimestamp,
    NonFiniteSetpoint,
    NonFiniteHysteresis,
    NegativeHysteresis,
    NonFiniteMeasurement,
    NonFiniteOutputOff,
    NonFiniteOutputOn,
    NonFiniteThreshold,
}

impl fmt::Display for OnOffControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteTimestamp => {
                formatter.write_str("On/off input timestamp must be finite")
            }

            Self::NonFiniteSetpoint => formatter.write_str("On/off setpoint must be finite"),

            Self::NonFiniteHysteresis => formatter.write_str("On/off hysteresis must be finite"),

            Self::NegativeHysteresis => {
                formatter.write_str("On/off hysteresis must be non-negative")
            }

            Self::NonFiniteMeasurement => formatter.write_str("On/off measurement must be finite"),

            Self::NonFiniteOutputOff => {
                formatter.write_str("On/off output-off value must be finite")
            }

            Self::NonFiniteOutputOn => formatter.write_str("On/off output-on value must be finite"),

            Self::NonFiniteThreshold => {
                formatter.write_str("On/off hysteresis thresholds must be finite")
            }
        }
    }
}

impl Error for OnOffControllerError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OnOffParameter {
    Setpoint,
    Hysteresis,
    OutputOff,
    OutputOn,
}

impl OnOffParameter {
    const ALL: [Self; 4] = [
        Self::Setpoint,
        Self::Hysteresis,
        Self::OutputOff,
        Self::OutputOn,
    ];
    fn key(self) -> &'static str {
        self.descriptor().key
    }
    pub(super) fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|parameter| parameter.key() == key)
    }
    fn descriptor(self) -> ParameterDescriptor {
        match self {
            Self::Setpoint => ParameterDescriptor {
                key: "setpoint",
                name: "setpoint",
                access: ParameterAccess::ReadWrite,
                value_type: ParameterValueType::Number,
                range: ParameterRange::Number {
                    minimum: -f64::MAX,
                    maximum: f64::MAX,
                },
            },
            Self::Hysteresis => ParameterDescriptor {
                key: "hysteresis",
                name: "hysteresis",
                access: ParameterAccess::ReadWrite,
                value_type: ParameterValueType::Number,
                range: ParameterRange::Number {
                    minimum: 0.0,
                    maximum: f64::MAX,
                },
            },
            Self::OutputOff => ParameterDescriptor {
                key: "output_off",
                name: "output off",
                access: ParameterAccess::ReadWrite,
                value_type: ParameterValueType::Number,
                range: ParameterRange::Number {
                    minimum: -f64::MAX,
                    maximum: f64::MAX,
                },
            },
            Self::OutputOn => ParameterDescriptor {
                key: "output_on",
                name: "output on",
                access: ParameterAccess::ReadWrite,
                value_type: ParameterValueType::Number,
                range: ParameterRange::Number {
                    minimum: -f64::MAX,
                    maximum: f64::MAX,
                },
            },
        }
    }
}

impl OnOffController {
    pub fn parameters(&self) -> Vec<ParameterDescriptor> {
        OnOffParameter::ALL
            .into_iter()
            .filter(|parameter| *parameter != OnOffParameter::Setpoint)
            .map(OnOffParameter::descriptor)
            .collect()
    }

    pub fn parameter_values(
        &self,
    ) -> Result<Vec<(String, InstrumentValue)>, ControllerParameterError> {
        OnOffParameter::ALL
            .into_iter()
            .map(|parameter| {
                let key = parameter.key();
                self.read_parameter(key)
                    .map(|value| (key.to_owned(), value))
            })
            .collect()
    }

    pub fn read_parameter(&self, key: &str) -> Result<InstrumentValue, ControllerParameterError> {
        let parameter = OnOffParameter::from_key(key)
            .ok_or_else(|| unknown_parameter(ControllerKind::OnOff, key))?;
        if !parameter.descriptor().access.readable() {
            return Err(ControllerParameterError::NotReadable(key.to_owned()));
        }

        let value = match parameter {
            OnOffParameter::Setpoint => self.setpoint(),

            OnOffParameter::Hysteresis => self.hysteresis(),

            OnOffParameter::OutputOff => self.output_off(),

            OnOffParameter::OutputOn => self.output_on(),
        };

        Ok(InstrumentValue::Number(value))
    }

    pub fn configure_parameters<I, K>(&mut self, updates: I) -> Result<(), ControllerParameterError>
    where
        I: IntoIterator<Item = (K, InstrumentValue)>,
        K: AsRef<str>,
    {
        let mut resolved = Vec::new();
        for (key, value) in updates {
            let key = key.as_ref();
            let parameter = OnOffParameter::from_key(key)
                .ok_or_else(|| unknown_parameter(ControllerKind::OnOff, key))?;
            if !parameter.descriptor().access.writable() {
                return Err(ControllerParameterError::NotWritable(key.to_owned()));
            }
            if resolved.iter().any(|(existing, _)| *existing == parameter) {
                return Err(ControllerParameterError::DuplicateParameter(key.to_owned()));
            }
            resolved.push((parameter, expect_number(key, value)?));
        }

        let mut setpoint = self.setpoint();

        let mut hysteresis = self.hysteresis();

        let mut output_off = self.output_off();

        let mut output_on = self.output_on();

        for (parameter, value) in &resolved {
            match parameter {
                OnOffParameter::Setpoint => {
                    setpoint = *value;
                }

                OnOffParameter::Hysteresis => {
                    hysteresis = *value;
                }

                OnOffParameter::OutputOff => {
                    output_off = *value;
                }

                OnOffParameter::OutputOn => {
                    output_on = *value;
                }
            }
        }

        self.configure(setpoint, hysteresis, output_off, output_on)
            .map_err(ControllerParameterError::OnOff)
    }
}

#[cfg(test)]
mod tests {
    use super::{OnOffController, OnOffControllerError};

    #[test]
    fn starts_in_off_state() {
        let mut controller = controller();

        let output = controller.update(0.0, 100.0).unwrap();

        assert!(!output.active());
        assert_eq!(output.value(), 0.0);
    }

    #[test]
    fn switches_using_hysteresis() {
        let mut controller = controller();

        let below = controller.update(0.0, 97.0).unwrap();

        assert!(below.active());
        assert_eq!(below.value(), 100.0);

        let inside_after_on = controller.update(1.0, 100.0).unwrap();

        assert!(inside_after_on.active());
        assert_eq!(inside_after_on.value(), 100.0,);

        let above = controller.update(2.0, 103.0).unwrap();

        assert!(!above.active());
        assert_eq!(above.value(), 0.0);

        let inside_after_off = controller.update(3.0, 100.0).unwrap();

        assert!(!inside_after_off.active());
        assert_eq!(inside_after_off.value(), 0.0,);
    }

    #[test]
    fn retains_state_at_hysteresis_boundaries() {
        let mut controller = controller();

        controller.update(0.0, 97.0).unwrap();

        let lower = controller.update(1.0, 98.0).unwrap();

        assert!(lower.active());

        let upper = controller.update(2.0, 102.0).unwrap();

        assert!(upper.active());

        let above = controller.update(3.0, 102.1).unwrap();

        assert!(!above.active());
    }

    #[test]
    fn reset_returns_controller_to_off_state() {
        let mut controller = controller();

        controller.update(0.0, 97.0).unwrap();

        assert!(controller.active());

        controller.reset();

        assert!(!controller.active());

        let output = controller.update(1.0, 100.0).unwrap();

        assert!(!output.active());
        assert_eq!(output.value(), 0.0);
    }

    #[test]
    fn configure_preserves_runtime_state() {
        let mut controller = controller();

        controller.update(0.0, 97.0).unwrap();

        assert!(controller.active());

        controller.configure(150.0, 5.0, 10.0, 80.0).unwrap();

        assert!(controller.active());

        assert_eq!(controller.setpoint(), 150.0,);

        assert_eq!(controller.hysteresis(), 5.0,);

        assert_eq!(controller.output_off(), 10.0,);

        assert_eq!(controller.output_on(), 80.0,);
    }

    #[test]
    fn invalid_configuration_is_atomic() {
        let mut controller = controller();

        controller.update(0.0, 97.0).unwrap();

        let error = controller.configure(150.0, -1.0, 10.0, 80.0).unwrap_err();

        assert_eq!(error, OnOffControllerError::NegativeHysteresis,);

        assert_eq!(controller.setpoint(), 100.0,);

        assert_eq!(controller.hysteresis(), 2.0,);

        assert_eq!(controller.output_off(), 0.0,);

        assert_eq!(controller.output_on(), 100.0,);

        assert!(controller.active());
    }

    #[test]
    fn rejects_non_finite_input() {
        let mut controller = controller();

        assert_eq!(
            controller.update(f64::NAN, 100.0).unwrap_err(),
            OnOffControllerError::NonFiniteTimestamp,
        );

        assert_eq!(
            controller.update(0.0, f64::NAN).unwrap_err(),
            OnOffControllerError::NonFiniteMeasurement,
        );
    }

    #[test]
    fn rejects_invalid_configuration() {
        assert_eq!(
            OnOffController::new(100.0, -1.0, 0.0, 100.0,).unwrap_err(),
            OnOffControllerError::NegativeHysteresis,
        );

        assert_eq!(
            OnOffController::new(f64::NAN, 2.0, 0.0, 100.0,).unwrap_err(),
            OnOffControllerError::NonFiniteSetpoint,
        );

        assert_eq!(
            OnOffController::new(100.0, 2.0, f64::NAN, 100.0,).unwrap_err(),
            OnOffControllerError::NonFiniteOutputOff,
        );

        assert_eq!(
            OnOffController::new(100.0, 2.0, 0.0, f64::NAN,).unwrap_err(),
            OnOffControllerError::NonFiniteOutputOn,
        );
    }

    fn controller() -> OnOffController {
        OnOffController::new(100.0, 2.0, 0.0, 100.0).unwrap()
    }
}
