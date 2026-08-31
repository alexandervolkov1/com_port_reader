use std::{error::Error, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceKind {
    Fixed,
    Ramp,
}

impl ReferenceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Ramp => "ramp",
        }
    }
}

impl fmt::Display for ReferenceKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixedReference {
    value: f64,
}

impl FixedReference {
    pub fn new(value: f64) -> Result<Self, ReferenceSourceError> {
        if !value.is_finite() {
            return Err(ReferenceSourceError::NonFiniteFixedValue);
        }

        Ok(Self { value })
    }

    pub const fn value(self) -> f64 {
        self.value
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RampReference {
    start: f64,
    target: f64,
    rate: f64,
}

impl RampReference {
    pub fn new(start: f64, target: f64, rate: f64) -> Result<Self, ReferenceSourceError> {
        if !start.is_finite() {
            return Err(ReferenceSourceError::NonFiniteRampStart);
        }

        if !target.is_finite() {
            return Err(ReferenceSourceError::NonFiniteRampTarget);
        }

        if !rate.is_finite() {
            return Err(ReferenceSourceError::NonFiniteRampRate);
        }

        if rate <= 0.0 {
            return Err(ReferenceSourceError::NonPositiveRampRate);
        }

        let span = target - start;

        if !span.is_finite() {
            return Err(ReferenceSourceError::NonFiniteRampSpan);
        }

        Ok(Self {
            start,
            target,
            rate,
        })
    }

    pub const fn start(self) -> f64 {
        self.start
    }

    pub const fn target(self) -> f64 {
        self.target
    }

    pub const fn rate(self) -> f64 {
        self.rate
    }

    pub fn value_at_elapsed(self, elapsed_seconds: f64) -> Result<f64, ReferenceSourceError> {
        if !elapsed_seconds.is_finite() {
            return Err(ReferenceSourceError::NonFiniteElapsedTime);
        }

        if elapsed_seconds < 0.0 {
            return Err(ReferenceSourceError::NegativeElapsedTime);
        }

        let span = self.target - self.start;

        if span == 0.0 {
            return Ok(self.target);
        }

        let distance = span.abs();

        let duration = distance / self.rate;

        if elapsed_seconds >= duration {
            return Ok(self.target);
        }

        let direction = span.signum();

        Ok(self.start + direction * self.rate * elapsed_seconds)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReferenceSource {
    Fixed(FixedReference),
    Ramp(RampReference),
}

impl ReferenceSource {
    pub fn fixed(value: f64) -> Result<Self, ReferenceSourceError> {
        FixedReference::new(value).map(Self::Fixed)
    }

    pub fn ramp(start: f64, target: f64, rate: f64) -> Result<Self, ReferenceSourceError> {
        RampReference::new(start, target, rate).map(Self::Ramp)
    }

    pub const fn kind(self) -> ReferenceKind {
        match self {
            Self::Fixed(_) => ReferenceKind::Fixed,

            Self::Ramp(_) => ReferenceKind::Ramp,
        }
    }

    pub fn value_at_elapsed(self, elapsed_seconds: f64) -> Result<f64, ReferenceSourceError> {
        match self {
            Self::Fixed(reference) => Ok(reference.value()),

            Self::Ramp(reference) => reference.value_at_elapsed(elapsed_seconds),
        }
    }
}

impl From<FixedReference> for ReferenceSource {
    fn from(reference: FixedReference) -> Self {
        Self::Fixed(reference)
    }
}

impl From<RampReference> for ReferenceSource {
    fn from(reference: RampReference) -> Self {
        Self::Ramp(reference)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceSourceError {
    NonFiniteFixedValue,
    NonFiniteRampStart,
    NonFiniteRampTarget,
    NonFiniteRampRate,
    NonPositiveRampRate,
    NonFiniteRampSpan,
    NonFiniteElapsedTime,
    NegativeElapsedTime,
}

impl fmt::Display for ReferenceSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteFixedValue => formatter.write_str(
                "Fixed reference value \
                     must be finite",
            ),

            Self::NonFiniteRampStart => formatter.write_str(
                "Ramp start value \
                     must be finite",
            ),

            Self::NonFiniteRampTarget => formatter.write_str(
                "Ramp target value \
                     must be finite",
            ),

            Self::NonFiniteRampRate => formatter.write_str("Ramp rate must be finite"),

            Self::NonPositiveRampRate => formatter.write_str(
                "Ramp rate must be \
                     greater than zero",
            ),

            Self::NonFiniteRampSpan => formatter.write_str("Ramp span must be finite"),

            Self::NonFiniteElapsedTime => formatter.write_str(
                "Reference elapsed time \
                     must be finite",
            ),

            Self::NegativeElapsedTime => formatter.write_str(
                "Reference elapsed time \
                     must not be negative",
            ),
        }
    }
}

impl Error for ReferenceSourceError {}

#[cfg(test)]
mod tests {
    use super::{
        FixedReference, RampReference, ReferenceKind, ReferenceSource, ReferenceSourceError,
    };

    #[test]
    fn fixed_reference_returns_constant_value() {
        let reference = FixedReference::new(150.0).unwrap();

        assert_eq!(reference.value(), 150.0,);
    }

    #[test]
    fn upward_ramp_reaches_and_holds_target() {
        let reference = RampReference::new(20.0, 100.0, 10.0).unwrap();

        assert_eq!(reference.value_at_elapsed(0.0).unwrap(), 20.0,);

        assert_eq!(reference.value_at_elapsed(3.0).unwrap(), 50.0,);

        assert_eq!(reference.value_at_elapsed(8.0).unwrap(), 100.0,);

        assert_eq!(reference.value_at_elapsed(20.0).unwrap(), 100.0,);
    }

    #[test]
    fn downward_ramp_reaches_and_holds_target() {
        let reference = RampReference::new(100.0, 20.0, 10.0).unwrap();

        assert_eq!(reference.value_at_elapsed(0.0).unwrap(), 100.0,);

        assert_eq!(reference.value_at_elapsed(3.0).unwrap(), 70.0,);

        assert_eq!(reference.value_at_elapsed(8.0).unwrap(), 20.0,);

        assert_eq!(reference.value_at_elapsed(30.0).unwrap(), 20.0,);
    }

    #[test]
    fn zero_length_ramp_holds_target() {
        let reference = RampReference::new(100.0, 100.0, 10.0).unwrap();

        assert_eq!(reference.value_at_elapsed(0.0).unwrap(), 100.0,);

        assert_eq!(reference.value_at_elapsed(1_000.0).unwrap(), 100.0,);
    }

    #[test]
    fn reference_source_dispatches_by_kind() {
        let fixed = ReferenceSource::fixed(150.0).unwrap();

        assert_eq!(fixed.kind(), ReferenceKind::Fixed,);

        assert_eq!(fixed.value_at_elapsed(10.0).unwrap(), 150.0,);

        let ramp = ReferenceSource::ramp(20.0, 100.0, 10.0).unwrap();

        assert_eq!(ramp.kind(), ReferenceKind::Ramp,);

        assert_eq!(ramp.value_at_elapsed(2.0).unwrap(), 40.0,);
    }

    #[test]
    fn rejects_invalid_reference_definitions() {
        assert_eq!(
            FixedReference::new(f64::NAN,),
            Err(ReferenceSourceError::NonFiniteFixedValue,),
        );

        assert_eq!(
            RampReference::new(f64::NAN, 100.0, 1.0,),
            Err(ReferenceSourceError::NonFiniteRampStart,),
        );

        assert_eq!(
            RampReference::new(20.0, f64::INFINITY, 1.0,),
            Err(ReferenceSourceError::NonFiniteRampTarget,),
        );

        assert_eq!(
            RampReference::new(20.0, 100.0, f64::NAN,),
            Err(ReferenceSourceError::NonFiniteRampRate,),
        );

        assert_eq!(
            RampReference::new(20.0, 100.0, 0.0,),
            Err(ReferenceSourceError::NonPositiveRampRate,),
        );

        assert_eq!(
            RampReference::new(20.0, 100.0, -1.0,),
            Err(ReferenceSourceError::NonPositiveRampRate,),
        );

        assert_eq!(
            RampReference::new(-f64::MAX, f64::MAX, 1.0,),
            Err(ReferenceSourceError::NonFiniteRampSpan,),
        );
    }

    #[test]
    fn rejects_invalid_ramp_elapsed_time() {
        let reference = RampReference::new(20.0, 100.0, 10.0).unwrap();

        assert_eq!(
            reference.value_at_elapsed(f64::NAN,),
            Err(ReferenceSourceError::NonFiniteElapsedTime,),
        );

        assert_eq!(
            reference.value_at_elapsed(-1.0),
            Err(ReferenceSourceError::NegativeElapsedTime,),
        );
    }
}
