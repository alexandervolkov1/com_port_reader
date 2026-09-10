use mlua::{FromLua, Lua, Value};

use crate::instrument::{
    InstrumentValue, ParameterDescriptor, ParameterValueType, metakon_5x3::Metakon5x3Register,
    virtual_instrument::VirtualParameterDescriptor,
};

pub(super) fn scaled_integer_parameter_value<T>(
    lua: &Lua,
    parameter: Metakon5x3Register,
    value: Value,
    scale: f64,
) -> mlua::Result<T>
where
    T: TryFrom<i64>,
{
    let engineering_value = f64::from_lua(value, lua).map_err(|_| {
        mlua::Error::RuntimeError(format!(
            "Metakon 5X3 parameter '{}' expects \
                 a numeric value",
            parameter.descriptor().key,
        ))
    })?;

    if !engineering_value.is_finite() {
        return Err(mlua::Error::RuntimeError(format!(
            "Metakon 5X3 parameter '{}' must \
                 be finite",
            parameter.descriptor().key,
        )));
    }

    let raw_value = engineering_value / scale;
    let rounded_value = raw_value.round();

    let tolerance = raw_value.abs().max(1.0) * 1.0e-9;

    if (raw_value - rounded_value).abs() > tolerance {
        return Err(mlua::Error::RuntimeError(format!(
            "Value {engineering_value} cannot be \
                 represented by Metakon 5X3 parameter \
                 '{}' with scale {scale}",
            parameter.descriptor().key,
        )));
    }

    let raw_value = rounded_value as i64;

    T::try_from(raw_value).map_err(|_| {
        mlua::Error::RuntimeError(format!(
            "Raw value {raw_value} does not fit \
             Metakon 5X3 parameter '{}'",
            parameter.descriptor().key,
        ))
    })
}

pub(super) fn boolean_parameter_value(
    lua: &Lua,
    parameter: Metakon5x3Register,
    value: Value,
) -> mlua::Result<bool> {
    bool::from_lua(value, lua).map_err(|_| {
        mlua::Error::RuntimeError(format!(
            "Metakon 5X3 parameter '{}' expects \
             a Boolean value",
            parameter.descriptor().key,
        ))
    })
}

pub(super) fn instrument_value_to_lua(value: InstrumentValue) -> Value {
    match value {
        InstrumentValue::Boolean(value) => Value::Boolean(value),

        InstrumentValue::Integer(value) => Value::Integer(value),

        InstrumentValue::Number(value) => Value::Number(value),
    }
}

pub(super) fn controller_value_from_lua(
    lua: &Lua,
    parameter: ParameterDescriptor,
    value: Value,
) -> mlua::Result<InstrumentValue> {
    match parameter.value_type {
        ParameterValueType::Boolean => {
            let value = bool::from_lua(value, lua).map_err(|_| {
                mlua::Error::RuntimeError(format!(
                    "Controller parameter '{}' expects a Boolean value",
                    parameter.key,
                ))
            })?;

            Ok(InstrumentValue::Boolean(value))
        }

        ParameterValueType::Integer => {
            let value = i64::from_lua(value, lua).map_err(|_| {
                mlua::Error::RuntimeError(format!(
                    "Controller parameter '{}' expects an integer value",
                    parameter.key,
                ))
            })?;

            Ok(InstrumentValue::Integer(value))
        }

        ParameterValueType::Number => {
            let value = f64::from_lua(value, lua).map_err(|_| {
                mlua::Error::RuntimeError(format!(
                    "Controller parameter '{}' expects a numeric value",
                    parameter.key,
                ))
            })?;

            if !value.is_finite() {
                return Err(mlua::Error::RuntimeError(format!(
                    "Controller parameter '{}' must be finite",
                    parameter.key,
                )));
            }

            Ok(InstrumentValue::Number(value))
        }
    }
}

pub(super) fn reference_value_from_lua(
    lua: &Lua,
    parameter: ParameterDescriptor,
    value: Value,
) -> mlua::Result<InstrumentValue> {
    match parameter.value_type {
        ParameterValueType::Boolean => {
            let value = bool::from_lua(value, lua).map_err(|_| {
                mlua::Error::RuntimeError(format!(
                    "Reference \
                                     parameter \
                                     '{}' expects \
                                     a Boolean value",
                    parameter.key,
                ))
            })?;

            Ok(InstrumentValue::Boolean(value))
        }

        ParameterValueType::Integer => {
            let value = i64::from_lua(value, lua).map_err(|_| {
                mlua::Error::RuntimeError(format!(
                    "Reference \
                                     parameter \
                                     '{}' expects \
                                     an integer value",
                    parameter.key,
                ))
            })?;

            Ok(InstrumentValue::Integer(value))
        }

        ParameterValueType::Number => {
            let value = f64::from_lua(value, lua).map_err(|_| {
                mlua::Error::RuntimeError(format!(
                    "Reference \
                                     parameter \
                                     '{}' expects \
                                     a numeric value",
                    parameter.key,
                ))
            })?;

            if !value.is_finite() {
                return Err(mlua::Error::RuntimeError(format!(
                    "Reference parameter \
                             '{}' must be finite",
                    parameter.key,
                )));
            }

            Ok(InstrumentValue::Number(value))
        }
    }
}

pub(super) fn virtual_instrument_value_from_lua(
    lua: &Lua,
    parameter: &VirtualParameterDescriptor,
    value: Value,
) -> mlua::Result<InstrumentValue> {
    let parameter_key = parameter.key();

    match parameter.value_type() {
        ParameterValueType::Boolean => {
            let value = bool::from_lua(value, lua).map_err(|_| {
                mlua::Error::RuntimeError(format!(
                    "Virtual instrument parameter \
                     '{parameter_key}' expects a \
                     Boolean value",
                ))
            })?;

            Ok(InstrumentValue::Boolean(value))
        }

        ParameterValueType::Integer => {
            let value = i64::from_lua(value, lua).map_err(|_| {
                mlua::Error::RuntimeError(format!(
                    "Virtual instrument parameter \
                     '{parameter_key}' expects an \
                     integer value",
                ))
            })?;

            Ok(InstrumentValue::Integer(value))
        }

        ParameterValueType::Number => {
            let value = f64::from_lua(value, lua).map_err(|_| {
                mlua::Error::RuntimeError(format!(
                    "Virtual instrument parameter \
                     '{parameter_key}' expects a \
                     numeric value",
                ))
            })?;

            if !value.is_finite() {
                return Err(mlua::Error::RuntimeError(format!(
                    "Virtual instrument parameter \
                         '{parameter_key}' must be \
                         finite",
                )));
            }

            Ok(InstrumentValue::Number(value))
        }
    }
}
