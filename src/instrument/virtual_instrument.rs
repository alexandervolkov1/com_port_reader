use std::collections::HashSet;

use super::{ParameterAccess, ParameterRange, ParameterValueType};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VirtualInstrumentId(u16);

impl VirtualInstrumentId {
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u16 {
        self.0
    }
}

impl std::fmt::Display for VirtualInstrumentId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VirtualParameterId(u16);

impl VirtualParameterId {
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u16 {
        self.0
    }
}

impl std::fmt::Display for VirtualParameterId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VirtualParameterDescriptor {
    id: VirtualParameterId,
    key: String,
    name: String,
    access: ParameterAccess,
    value_type: ParameterValueType,
    range: Option<ParameterRange>,
    unit: Option<String>,
    series: bool,
}

impl VirtualParameterDescriptor {
    pub fn new(
        id: VirtualParameterId,
        key: impl Into<String>,
        name: impl Into<String>,
        access: ParameterAccess,
        value_type: ParameterValueType,
    ) -> Self {
        Self {
            id,
            key: key.into().trim().to_owned(),
            name: name.into().trim().to_owned(),
            access,
            value_type,
            range: None,
            unit: None,
            series: false,
        }
    }

    pub fn with_range(mut self, range: ParameterRange) -> Self {
        self.range = Some(range);
        self
    }

    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into().trim().to_owned());

        self
    }

    pub fn with_series(mut self, series: bool) -> Self {
        self.series = series;
        self
    }

    pub const fn id(&self) -> VirtualParameterId {
        self.id
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn access(&self) -> ParameterAccess {
        self.access
    }

    pub const fn value_type(&self) -> ParameterValueType {
        self.value_type
    }

    pub const fn range(&self) -> Option<ParameterRange> {
        self.range
    }

    pub fn unit(&self) -> Option<&str> {
        self.unit.as_deref()
    }

    pub const fn series(&self) -> bool {
        self.series
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VirtualInstrumentDescriptor {
    id: VirtualInstrumentId,
    name: String,
    parameters: Vec<VirtualParameterDescriptor>,
}

impl VirtualInstrumentDescriptor {
    pub fn new(
        id: VirtualInstrumentId,
        name: impl Into<String>,
        parameters: Vec<VirtualParameterDescriptor>,
    ) -> Result<Self, VirtualInstrumentSchemaError> {
        let descriptor = Self {
            id,
            name: name.into().trim().to_owned(),
            parameters,
        };

        descriptor.validate()?;

        Ok(descriptor)
    }

    pub const fn id(&self) -> VirtualInstrumentId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn parameters(&self) -> &[VirtualParameterDescriptor] {
        &self.parameters
    }

    pub fn parameter_by_key(&self, key: &str) -> Option<&VirtualParameterDescriptor> {
        self.parameters
            .iter()
            .find(|parameter| parameter.key == key)
    }

    pub fn parameter_by_id(&self, id: VirtualParameterId) -> Option<&VirtualParameterDescriptor> {
        self.parameters.iter().find(|parameter| parameter.id == id)
    }

    fn validate(&self) -> Result<(), VirtualInstrumentSchemaError> {
        if self.name.is_empty() {
            return Err(VirtualInstrumentSchemaError::EmptyName);
        }

        if self.parameters.is_empty() {
            return Err(VirtualInstrumentSchemaError::NoParameters);
        }

        let mut parameter_ids = HashSet::new();
        let mut parameter_keys = HashSet::new();

        for parameter in &self.parameters {
            validate_parameter(parameter)?;

            if !parameter_ids.insert(parameter.id) {
                return Err(VirtualInstrumentSchemaError::DuplicateParameterId(
                    parameter.id,
                ));
            }

            if !parameter_keys.insert(parameter.key.clone()) {
                return Err(VirtualInstrumentSchemaError::DuplicateParameterKey(
                    parameter.key.clone(),
                ));
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VirtualInstrumentSchemaError {
    EmptyName,
    NoParameters,

    EmptyParameterKey { id: VirtualParameterId },

    InvalidParameterKey(String),
    EmptyParameterName(String),
    EmptyParameterUnit(String),

    DuplicateParameterId(VirtualParameterId),

    DuplicateParameterKey(String),
    InvalidRange(String),
    RangeTypeMismatch(String),

    SeriesParameterIsNotReadable(String),
}

impl std::fmt::Display for VirtualInstrumentSchemaError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyName => formatter.write_str("Virtual instrument name cannot be empty"),

            Self::NoParameters => formatter.write_str(
                "Virtual instrument must have at least \
                 one parameter",
            ),

            Self::EmptyParameterKey { id } => {
                write!(
                    formatter,
                    "Virtual parameter {id} key cannot \
                     be empty",
                )
            }

            Self::InvalidParameterKey(key) => {
                write!(
                    formatter,
                    "Invalid virtual parameter key \
                     '{key}'",
                )
            }

            Self::EmptyParameterName(key) => {
                write!(
                    formatter,
                    "Virtual parameter '{key}' name \
                     cannot be empty",
                )
            }

            Self::EmptyParameterUnit(key) => {
                write!(
                    formatter,
                    "Virtual parameter '{key}' unit \
                     cannot be empty",
                )
            }

            Self::DuplicateParameterId(id) => {
                write!(
                    formatter,
                    "Duplicate virtual parameter ID \
                     {id}",
                )
            }

            Self::DuplicateParameterKey(key) => {
                write!(
                    formatter,
                    "Duplicate virtual parameter key \
                     '{key}'",
                )
            }

            Self::InvalidRange(key) => {
                write!(
                    formatter,
                    "Invalid range for virtual \
                     parameter '{key}'",
                )
            }

            Self::RangeTypeMismatch(key) => {
                write!(
                    formatter,
                    "Range type does not match virtual \
                     parameter '{key}'",
                )
            }

            Self::SeriesParameterIsNotReadable(key) => {
                write!(
                    formatter,
                    "Virtual parameter '{key}' cannot \
                     be sampled because it is not \
                     readable",
                )
            }
        }
    }
}

impl std::error::Error for VirtualInstrumentSchemaError {}

fn validate_parameter(
    parameter: &VirtualParameterDescriptor,
) -> Result<(), VirtualInstrumentSchemaError> {
    if parameter.key.is_empty() {
        return Err(VirtualInstrumentSchemaError::EmptyParameterKey { id: parameter.id });
    }

    if !valid_parameter_key(&parameter.key) {
        return Err(VirtualInstrumentSchemaError::InvalidParameterKey(
            parameter.key.clone(),
        ));
    }

    if parameter.name.is_empty() {
        return Err(VirtualInstrumentSchemaError::EmptyParameterName(
            parameter.key.clone(),
        ));
    }

    if parameter.unit.as_deref() == Some("") {
        return Err(VirtualInstrumentSchemaError::EmptyParameterUnit(
            parameter.key.clone(),
        ));
    }

    if parameter.series && !parameter.access.readable() {
        return Err(VirtualInstrumentSchemaError::SeriesParameterIsNotReadable(
            parameter.key.clone(),
        ));
    }

    validate_parameter_range(parameter)
}

fn validate_parameter_range(
    parameter: &VirtualParameterDescriptor,
) -> Result<(), VirtualInstrumentSchemaError> {
    match (parameter.value_type, parameter.range) {
        (_, None) => Ok(()),

        (ParameterValueType::Integer, Some(ParameterRange::Integer { minimum, maximum }))
            if minimum <= maximum =>
        {
            Ok(())
        }

        (ParameterValueType::Number, Some(ParameterRange::Number { minimum, maximum }))
            if minimum.is_finite() && maximum.is_finite() && minimum <= maximum =>
        {
            Ok(())
        }

        (ParameterValueType::Integer, Some(ParameterRange::Integer { .. }))
        | (ParameterValueType::Number, Some(ParameterRange::Number { .. })) => Err(
            VirtualInstrumentSchemaError::InvalidRange(parameter.key.clone()),
        ),

        _ => Err(VirtualInstrumentSchemaError::RangeTypeMismatch(
            parameter.key.clone(),
        )),
    }
}

fn valid_parameter_key(key: &str) -> bool {
    let mut characters = key.chars();

    let Some(first) = characters.next() else {
        return false;
    };

    if first != '_' && !first.is_ascii_alphabetic() {
        return false;
    }

    characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::{
        VirtualInstrumentDescriptor, VirtualInstrumentId, VirtualInstrumentSchemaError,
        VirtualParameterDescriptor, VirtualParameterId,
    };

    use crate::instrument::{ParameterAccess, ParameterRange, ParameterValueType};

    fn number_parameter(id: u16, key: &str) -> VirtualParameterDescriptor {
        VirtualParameterDescriptor::new(
            VirtualParameterId::new(id),
            key,
            key,
            ParameterAccess::ReadWrite,
            ParameterValueType::Number,
        )
    }

    #[test]
    fn creates_dynamic_instrument_descriptor() {
        let value = number_parameter(1, "value")
            .with_range(ParameterRange::Number {
                minimum: -100.0,
                maximum: 100.0,
            })
            .with_unit("V")
            .with_series(true);

        let descriptor = VirtualInstrumentDescriptor::new(
            VirtualInstrumentId::new(7),
            "Wave generator",
            vec![value],
        )
        .unwrap();

        assert_eq!(descriptor.id(), VirtualInstrumentId::new(7),);

        assert_eq!(descriptor.name(), "Wave generator",);

        let parameter = descriptor.parameter_by_key("value").unwrap();

        assert_eq!(parameter.id(), VirtualParameterId::new(1),);

        assert_eq!(parameter.unit(), Some("V"));
        assert!(parameter.series());
    }

    #[test]
    fn rejects_duplicate_parameter_ids() {
        let result = VirtualInstrumentDescriptor::new(
            VirtualInstrumentId::new(1),
            "Generator",
            vec![
                number_parameter(1, "value"),
                number_parameter(1, "amplitude"),
            ],
        );

        assert_eq!(
            result,
            Err(VirtualInstrumentSchemaError::DuplicateParameterId(
                VirtualParameterId::new(1),
            ),),
        );
    }

    #[test]
    fn rejects_duplicate_parameter_keys() {
        let result = VirtualInstrumentDescriptor::new(
            VirtualInstrumentId::new(1),
            "Generator",
            vec![number_parameter(1, "value"), number_parameter(2, "value")],
        );

        assert_eq!(
            result,
            Err(VirtualInstrumentSchemaError::DuplicateParameterKey(
                "value".to_owned(),
            ),),
        );
    }

    #[test]
    fn rejects_invalid_parameter_key() {
        let result = VirtualInstrumentDescriptor::new(
            VirtualInstrumentId::new(1),
            "Generator",
            vec![number_parameter(1, "output-power")],
        );

        assert_eq!(
            result,
            Err(VirtualInstrumentSchemaError::InvalidParameterKey(
                "output-power".to_owned(),
            ),),
        );
    }

    #[test]
    fn rejects_invalid_number_range() {
        let parameter = number_parameter(1, "value").with_range(ParameterRange::Number {
            minimum: f64::NAN,
            maximum: 100.0,
        });

        let result = VirtualInstrumentDescriptor::new(
            VirtualInstrumentId::new(1),
            "Generator",
            vec![parameter],
        );

        assert_eq!(
            result,
            Err(VirtualInstrumentSchemaError::InvalidRange(
                "value".to_owned(),
            ),),
        );
    }

    #[test]
    fn rejects_range_type_mismatch() {
        let parameter = number_parameter(1, "value").with_range(ParameterRange::Integer {
            minimum: 0,
            maximum: 100,
        });

        let result = VirtualInstrumentDescriptor::new(
            VirtualInstrumentId::new(1),
            "Generator",
            vec![parameter],
        );

        assert_eq!(
            result,
            Err(VirtualInstrumentSchemaError::RangeTypeMismatch(
                "value".to_owned(),
            ),),
        );
    }

    #[test]
    fn rejects_write_only_series_parameter() {
        let parameter = VirtualParameterDescriptor::new(
            VirtualParameterId::new(1),
            "reset",
            "Reset",
            ParameterAccess::WriteOnly,
            ParameterValueType::Boolean,
        )
        .with_series(true);

        let result = VirtualInstrumentDescriptor::new(
            VirtualInstrumentId::new(1),
            "Generator",
            vec![parameter],
        );

        assert_eq!(
            result,
            Err(VirtualInstrumentSchemaError::SeriesParameterIsNotReadable(
                "reset".to_owned(),
            ),),
        );
    }
}
