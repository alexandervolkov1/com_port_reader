use std::time::Duration;

use mlua::{Function, Lua, Table, Value};

use crate::{
    instrument::{
        InstrumentValue, ParameterAccess, ParameterRange, ParameterValueType,
        virtual_instrument::{
            VirtualInstrumentDescriptor, VirtualInstrumentId, VirtualParameterDescriptor,
            VirtualParameterId,
        },
    },
    lua_execution::run_with_limit,
    protocol::virtual_instrument::{VirtualInstrumentModel, VirtualInstrumentModelError},
};

pub struct LuaVirtualInstrumentModel {
    lua: Lua,
    instruments: Vec<VirtualInstrumentDescriptor>,
}

impl LuaVirtualInstrumentModel {
    pub fn from_source(source: &str) -> Result<Self, VirtualInstrumentModelError> {
        let lua = Lua::new();

        let instruments = run_with_limit(&lua, || {
            lua.load(source).exec()?;

            let table = lua.globals().get::<Table>("instruments").map_err(|error| {
                mlua::Error::RuntimeError(format!(
                    "Lua virtual instrument \
                                 model must define global \
                                 table 'instruments': \
                                 {error}",
                ))
            })?;

            let instruments = parse_instruments(&table)?;

            validate_handlers(&lua, &instruments)?;

            Ok(instruments)
        })
        .map_err(|error| {
            VirtualInstrumentModelError::from(format!(
                "Failed to load Lua virtual \
                         instrument model: {error}",
            ))
        })?;

        Ok(Self { lua, instruments })
    }

    fn parameter_details(
        &self,
        instrument_id: VirtualInstrumentId,
        parameter_id: VirtualParameterId,
    ) -> Result<(String, ParameterValueType), VirtualInstrumentModelError> {
        let instrument = self
            .instruments
            .iter()
            .find(|instrument| instrument.id() == instrument_id)
            .ok_or_else(|| {
                VirtualInstrumentModelError::from(format!(
                    "Unknown virtual instrument \
                         ID {instrument_id}",
                ))
            })?;

        let parameter = instrument.parameter_by_id(parameter_id).ok_or_else(|| {
            VirtualInstrumentModelError::from(format!(
                "Unknown parameter ID \
                         {parameter_id} for virtual \
                         instrument {instrument_id}",
            ))
        })?;

        Ok((parameter.key().to_owned(), parameter.value_type()))
    }
}

impl VirtualInstrumentModel for LuaVirtualInstrumentModel {
    fn instruments(&self) -> &[VirtualInstrumentDescriptor] {
        &self.instruments
    }

    fn read(
        &mut self,
        instrument: VirtualInstrumentId,
        parameter: VirtualParameterId,
        elapsed: Duration,
    ) -> Result<InstrumentValue, VirtualInstrumentModelError> {
        let (parameter_key, value_type) = self.parameter_details(instrument, parameter)?;

        run_with_limit(&self.lua, || {
            let read = self.lua.globals().get::<Function>("read")?;

            let value = read.call::<Value>((
                i64::from(instrument.value()),
                parameter_key,
                elapsed.as_secs_f64(),
            ))?;

            lua_value_to_instrument_value(value, value_type)
        })
        .map_err(|error| {
            VirtualInstrumentModelError::from(format!(
                "Lua virtual instrument read \
                     failed: {error}",
            ))
        })
    }

    fn write(
        &mut self,
        instrument: VirtualInstrumentId,
        parameter: VirtualParameterId,
        value: InstrumentValue,
        elapsed: Duration,
    ) -> Result<InstrumentValue, VirtualInstrumentModelError> {
        let (parameter_key, value_type) = self.parameter_details(instrument, parameter)?;

        run_with_limit(&self.lua, || {
            let write = self.lua.globals().get::<Function>("write")?;

            let value = write.call::<Value>((
                i64::from(instrument.value()),
                parameter_key,
                instrument_value_to_lua(value),
                elapsed.as_secs_f64(),
            ))?;

            lua_value_to_instrument_value(value, value_type)
        })
        .map_err(|error| {
            VirtualInstrumentModelError::from(format!(
                "Lua virtual instrument write \
                     failed: {error}",
            ))
        })
    }
}

fn parse_instruments(table: &Table) -> mlua::Result<Vec<VirtualInstrumentDescriptor>> {
    let mut instruments = Vec::new();

    for (index, value) in table.sequence_values::<Table>().enumerate() {
        let table = value?;

        let position = index + 1;

        let id = u16::try_from(position)
            .map_err(|_| mlua::Error::RuntimeError("Too many virtual instruments".to_owned()))?;

        instruments.push(parse_instrument(VirtualInstrumentId::new(id), &table)?);
    }

    if instruments.is_empty() {
        return Err(mlua::Error::RuntimeError(
            "Lua virtual instrument model must \
                 define at least one instrument"
                .to_owned(),
        ));
    }

    Ok(instruments)
}

fn parse_instrument(
    id: VirtualInstrumentId,
    table: &Table,
) -> mlua::Result<VirtualInstrumentDescriptor> {
    let name = table.get::<String>("name")?;

    let parameter_table = table.get::<Table>("parameters")?;

    let mut parameters = Vec::new();

    for (index, value) in parameter_table.sequence_values::<Table>().enumerate() {
        let table = value?;

        let position = index + 1;

        let parameter_id = u16::try_from(position).map_err(|_| {
            mlua::Error::RuntimeError(format!(
                "Too many parameters in \
                             virtual instrument \
                             '{name}'",
            ))
        })?;

        parameters.push(parse_parameter(
            VirtualParameterId::new(parameter_id),
            &table,
        )?);
    }

    VirtualInstrumentDescriptor::new(id, name, parameters)
        .map_err(|error| mlua::Error::RuntimeError(error.to_string()))
}

fn parse_parameter(
    id: VirtualParameterId,
    table: &Table,
) -> mlua::Result<VirtualParameterDescriptor> {
    let key = table.get::<String>("key")?;

    let name = table
        .get::<Option<String>>("name")?
        .unwrap_or_else(|| key.clone());

    let access = parse_access(
        table
            .get::<Option<String>>("access")?
            .as_deref()
            .unwrap_or("read_only"),
    )?;

    let value_type = parse_value_type(&table.get::<String>("type")?)?;

    let series = table.get::<Option<bool>>("series")?.unwrap_or(false);

    let unit = table.get::<Option<String>>("unit")?;

    let range = parse_range(table, value_type)?;

    let mut descriptor =
        VirtualParameterDescriptor::new(id, key, name, access, value_type).with_series(series);

    if let Some(unit) = unit {
        descriptor = descriptor.with_unit(unit);
    }

    if let Some(range) = range {
        descriptor = descriptor.with_range(range);
    }

    Ok(descriptor)
}

fn parse_access(access: &str) -> mlua::Result<ParameterAccess> {
    match access {
        "read_only" => Ok(ParameterAccess::ReadOnly),

        "write_only" => Ok(ParameterAccess::WriteOnly),

        "read_write" => Ok(ParameterAccess::ReadWrite),

        _ => Err(mlua::Error::RuntimeError(format!(
            "Unknown virtual parameter \
                     access '{access}'",
        ))),
    }
}

fn parse_value_type(value_type: &str) -> mlua::Result<ParameterValueType> {
    match value_type {
        "boolean" => Ok(ParameterValueType::Boolean),

        "integer" => Ok(ParameterValueType::Integer),

        "number" => Ok(ParameterValueType::Number),

        _ => Err(mlua::Error::RuntimeError(format!(
            "Unknown virtual parameter \
                     type '{value_type}'",
        ))),
    }
}

fn parse_range(
    table: &Table,
    value_type: ParameterValueType,
) -> mlua::Result<Option<ParameterRange>> {
    let minimum = table.get::<Value>("min")?;

    let maximum = table.get::<Value>("max")?;

    match (&minimum, &maximum) {
        (Value::Nil, Value::Nil) => {
            return Ok(None);
        }

        (Value::Nil, _) | (_, Value::Nil) => {
            return Err(mlua::Error::RuntimeError(
                "Virtual parameter range must \
                     contain both 'min' and 'max'"
                    .to_owned(),
            ));
        }

        _ => {}
    }

    match value_type {
        ParameterValueType::Boolean => Err(mlua::Error::RuntimeError(
            "Boolean virtual parameter cannot \
                 declare a numeric range"
                .to_owned(),
        )),

        ParameterValueType::Integer => Ok(Some(ParameterRange::Integer {
            minimum: lua_integer(minimum, "min")?,
            maximum: lua_integer(maximum, "max")?,
        })),

        ParameterValueType::Number => Ok(Some(ParameterRange::Number {
            minimum: lua_number(minimum, "min")?,
            maximum: lua_number(maximum, "max")?,
        })),
    }
}

fn lua_integer(value: Value, field: &str) -> mlua::Result<i64> {
    match value {
        Value::Integer(value) => Ok(value),

        value => Err(mlua::Error::RuntimeError(format!(
            "Virtual parameter field \
                     '{field}' must be an integer, \
                     received {value:?}",
        ))),
    }
}

fn lua_number(value: Value, field: &str) -> mlua::Result<f64> {
    match value {
        Value::Integer(value) => Ok(value as f64),

        Value::Number(value) => Ok(value),

        value => Err(mlua::Error::RuntimeError(format!(
            "Virtual parameter field \
                     '{field}' must be a number, \
                     received {value:?}",
        ))),
    }
}

fn validate_handlers(lua: &Lua, instruments: &[VirtualInstrumentDescriptor]) -> mlua::Result<()> {
    let has_readable_parameter = instruments
        .iter()
        .flat_map(|instrument| instrument.parameters())
        .any(|parameter| parameter.access().readable());

    let has_writable_parameter = instruments
        .iter()
        .flat_map(|instrument| instrument.parameters())
        .any(|parameter| parameter.access().writable());

    if has_readable_parameter {
        require_function(lua, "read")?;
    }

    if has_writable_parameter {
        require_function(lua, "write")?;
    }

    Ok(())
}

fn require_function(lua: &Lua, name: &str) -> mlua::Result<()> {
    lua.globals()
        .get::<Function>(name)
        .map(|_| ())
        .map_err(|error| {
            mlua::Error::RuntimeError(format!(
                "Lua virtual instrument model \
                     must define global function \
                     '{name}': {error}",
            ))
        })
}

fn lua_value_to_instrument_value(
    value: Value,
    expected_type: ParameterValueType,
) -> mlua::Result<InstrumentValue> {
    match (expected_type, value) {
        (ParameterValueType::Boolean, Value::Boolean(value)) => Ok(InstrumentValue::Boolean(value)),

        (ParameterValueType::Integer, Value::Integer(value)) => Ok(InstrumentValue::Integer(value)),

        (ParameterValueType::Number, Value::Integer(value)) => {
            Ok(InstrumentValue::Number(value as f64))
        }

        (ParameterValueType::Number, Value::Number(value)) => Ok(InstrumentValue::Number(value)),

        (expected_type, value) => Err(mlua::Error::RuntimeError(format!(
            "Lua virtual instrument returned \
                     {value:?}, expected {}",
            expected_type.as_str(),
        ))),
    }
}

fn instrument_value_to_lua(value: InstrumentValue) -> Value {
    match value {
        InstrumentValue::Boolean(value) => Value::Boolean(value),

        InstrumentValue::Integer(value) => Value::Integer(value),

        InstrumentValue::Number(value) => Value::Number(value),
    }
}

#[cfg(test)]
mod tests {
    use std::{f64::consts::FRAC_PI_2, time::Duration};

    use super::LuaVirtualInstrumentModel;

    use crate::{
        instrument::{
            InstrumentValue, ParameterAccess, ParameterRange,
            virtual_instrument::{VirtualInstrumentId, VirtualParameterId},
        },
        protocol::virtual_instrument::VirtualInstrumentModel,
    };

    const INSTRUMENT: VirtualInstrumentId = VirtualInstrumentId::new(1);

    const VALUE: VirtualParameterId = VirtualParameterId::new(1);

    const AMPLITUDE: VirtualParameterId = VirtualParameterId::new(2);

    #[test]
    fn loads_dynamic_instrument_schema() {
        let model = LuaVirtualInstrumentModel::from_source(
            r#"
                instruments = {
                    {
                        name = "Generator",

                        parameters = {
                            {
                                key = "value",
                                name = "Signal value",
                                type = "number",
                                access = "read_only",
                                series = true,
                                unit = "V",
                                min = -100,
                                max = 100,
                            },

                            {
                                key = "amplitude",
                                type = "number",
                                access = "read_write",
                            },
                        },
                    },
                }

                function read(
                    instrument,
                    parameter,
                    time
                )
                    return 0
                end

                function write(
                    instrument,
                    parameter,
                    value,
                    time
                )
                    return value
                end
                "#,
        )
        .unwrap();

        let instruments = model.instruments();

        assert_eq!(instruments.len(), 1);
        assert_eq!(instruments[0].id(), INSTRUMENT,);
        assert_eq!(instruments[0].name(), "Generator",);

        let value = instruments[0].parameter_by_key("value").unwrap();

        assert_eq!(value.access(), ParameterAccess::ReadOnly,);

        assert_eq!(value.unit(), Some("V"));
        assert!(value.series());

        assert_eq!(
            value.range(),
            Some(ParameterRange::Number {
                minimum: -100.0,
                maximum: 100.0,
            }),
        );

        assert_eq!(
            instruments[0].parameter_by_key("amplitude").unwrap().id(),
            AMPLITUDE,
        );
    }

    #[test]
    fn evaluates_dynamic_signal() {
        let mut model = LuaVirtualInstrumentModel::from_source(
            r#"
                instruments = {
                    {
                        name = "Sine",

                        parameters = {
                            {
                                key = "value",
                                type = "number",
                                series = true,
                            },
                        },
                    },
                }

                function read(
                    instrument,
                    parameter,
                    time
                )
                    return math.sin(time)
                end
                "#,
        )
        .unwrap();

        let value = model
            .read(INSTRUMENT, VALUE, Duration::from_secs_f64(FRAC_PI_2))
            .unwrap();

        let InstrumentValue::Number(value) = value else {
            panic!("expected number");
        };

        assert!((value - 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn writes_and_preserves_state() {
        let mut model = LuaVirtualInstrumentModel::from_source(
            r#"
                local amplitude = 1

                instruments = {
                    {
                        name = "Generator",

                        parameters = {
                            {
                                key = "value",
                                type = "number",
                            },

                            {
                                key = "amplitude",
                                type = "number",
                                access = "read_write",
                            },
                        },
                    },
                }

                function read(
                    instrument,
                    parameter,
                    time
                )
                    if parameter == "value" then
                        return amplitude * 2
                    end

                    return amplitude
                end

                function write(
                    instrument,
                    parameter,
                    value,
                    time
                )
                    amplitude = value
                    return amplitude
                end
                "#,
        )
        .unwrap();

        let written = model
            .write(
                INSTRUMENT,
                AMPLITUDE,
                InstrumentValue::Number(5.0),
                Duration::ZERO,
            )
            .unwrap();

        assert_eq!(written, InstrumentValue::Number(5.0),);

        let value = model.read(INSTRUMENT, VALUE, Duration::ZERO).unwrap();

        assert_eq!(value, InstrumentValue::Number(10.0),);
    }

    #[test]
    fn requires_read_handler() {
        let result = LuaVirtualInstrumentModel::from_source(
            r#"
                instruments = {
                    {
                        name = "Generator",

                        parameters = {
                            {
                                key = "value",
                                type = "number",
                            },
                        },
                    },
                }
                "#,
        );

        let error = result.err().unwrap().to_string();

        assert!(error.contains("must define global function 'read'",),);
    }

    #[test]
    fn does_not_require_write_handler_for_read_only_model() {
        LuaVirtualInstrumentModel::from_source(
            r#"
            instruments = {
                {
                    name = "Generator",

                    parameters = {
                        {
                            key = "value",
                            type = "number",
                        },
                    },
                },
            }

            function read(
                instrument,
                parameter,
                time
            )
                return 0
            end
            "#,
        )
        .unwrap();
    }

    #[test]
    fn reports_read_handler_error() {
        let mut model = LuaVirtualInstrumentModel::from_source(
            r#"
                instruments = {
                    {
                        name = "Generator",

                        parameters = {
                            {
                                key = "value",
                                type = "number",
                            },
                        },
                    },
                }

                function read(
                    instrument,
                    parameter,
                    time
                )
                    error("simulated failure")
                end
                "#,
        )
        .unwrap();

        let error = model.read(INSTRUMENT, VALUE, Duration::ZERO).unwrap_err();

        assert!(error.to_string().contains("simulated failure"),);
    }
}
