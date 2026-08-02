pub mod metakon_5x3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterAccess {
    ReadOnly,
    ReadWrite,
}

impl ParameterAccess {
    pub const fn writable(self) -> bool {
        matches!(self, Self::ReadWrite)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterValueType {
    Boolean,
    Unsigned8,
    Signed8,
    Unsigned16,
    Signed16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParameterRange {
    pub minimum: i64,
    pub maximum: i64,
}

impl ParameterRange {
    pub const fn new(minimum: i64, maximum: i64) -> Self {
        Self { minimum, maximum }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParameterDescriptor {
    pub name: &'static str,
    pub access: ParameterAccess,
    pub value_type: ParameterValueType,
    pub range: ParameterRange,
}
