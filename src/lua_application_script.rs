//! Declarative script panels and callback retention in the application Lua state.
//!
//! Registry tables keep callback closures alive. Widget updates are validated against registered
//! metadata before UI events are sent; re-registering a script ID replaces its table and panels.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

use crossbeam_channel::Sender;
use mlua::{Lua, Table, Value};

use crate::{
    control_panel::{ControlDefinition, ControlPanelDefinition},
    scenario::{
        ScenarioCallbackInvocation, ScenarioCallbackTrigger, ScenarioCondition,
        ScenarioConditionKind,
    },
};

const SCRIPT_REGISTRY_KEY: &str = "com_port_reader.application_scripts";

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LuaControlArgument {
    Number(f64),
    Boolean(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LuaControlValue {
    Text(String),
    Number(f64),
    Boolean(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LuaControlInvocation {
    script_id: String,
    panel_id: String,
    control_id: String,
    callback: String,
    argument: Option<LuaControlArgument>,
}

impl LuaControlInvocation {
    pub(crate) fn new(
        script_id: impl Into<String>,
        panel_id: impl Into<String>,
        control_id: impl Into<String>,
        callback: impl Into<String>,
        argument: Option<LuaControlArgument>,
    ) -> Self {
        Self {
            script_id: script_id.into(),
            panel_id: panel_id.into(),
            control_id: control_id.into(),
            callback: callback.into(),
            argument,
        }
    }

    pub(crate) fn script_id(&self) -> &str {
        &self.script_id
    }

    pub(crate) fn panel_id(&self) -> &str {
        &self.panel_id
    }

    pub(crate) fn control_id(&self) -> &str {
        &self.control_id
    }

    pub(crate) fn callback(&self) -> &str {
        &self.callback
    }

    pub(crate) const fn argument(&self) -> Option<LuaControlArgument> {
        self.argument
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LuaApplicationEvent {
    ScriptRegistered {
        script_id: String,
        panels: Vec<ControlPanelDefinition>,
    },

    ScriptUnregistered {
        script_id: String,
    },

    ControlCallbackSucceeded {
        invocation: LuaControlInvocation,
    },

    ControlCallbackFailed {
        invocation: LuaControlInvocation,
        error: String,
    },

    ControlValueChanged {
        script_id: String,
        panel_id: String,
        control_id: String,
        value: LuaControlValue,
    },

    ControlEnabledChanged {
        script_id: String,
        panel_id: String,
        control_id: String,
        enabled: bool,
        reason: Option<String>,
    },
}

pub(crate) fn invoke_control_callback(
    lua: &Lua,
    invocation: &LuaControlInvocation,
) -> mlua::Result<()> {
    let registry: Table = lua.named_registry_value(SCRIPT_REGISTRY_KEY)?;

    let script = registry
        .get::<Option<Table>>(invocation.script_id())?
        .ok_or_else(|| {
            runtime_error(format!(
                "Application script '{}' is not registered",
                invocation.script_id(),
            ))
        })?;

    let callback = script.get::<Value>(invocation.callback())?;

    let Value::Function(callback) = callback else {
        return Err(runtime_error(format!(
            "Application script '{}' callback '{}' is not a function",
            invocation.script_id(),
            invocation.callback(),
        )));
    };

    match invocation.argument() {
        None => callback.call::<()>(()),

        Some(LuaControlArgument::Number(value)) => callback.call::<()>(value),

        Some(LuaControlArgument::Boolean(value)) => callback.call::<()>(value),
    }
}

pub(crate) fn invoke_scenario_callback(
    lua: &Lua,
    invocation: &ScenarioCallbackInvocation,
) -> mlua::Result<()> {
    let callback_name = invocation.callback();
    let event = scenario_callback_event(lua, invocation)?;

    if let Value::Function(callback) = lua.globals().get::<Value>(callback_name)? {
        return callback.call(event);
    }

    let registry: Table = lua.named_registry_value(SCRIPT_REGISTRY_KEY)?;
    let mut resolved = None;

    for pair in registry.pairs::<String, Table>() {
        let (script_id, script) = pair?;
        let Value::Function(callback) = script.get::<Value>(callback_name)? else {
            continue;
        };

        if let Some((previous_script, _)) = &resolved {
            return Err(runtime_error(format!(
                "Scenario callback '{callback_name}' is ambiguous: it is defined by application scripts '{previous_script}' and '{script_id}'",
            )));
        }

        resolved = Some((script_id, callback));
    }

    let Some((_, callback)) = resolved else {
        return Err(runtime_error(format!(
            "Scenario callback '{callback_name}' was not found as a global function or in a registered application script",
        )));
    };

    callback.call(event)
}

fn scenario_callback_event(
    lua: &Lua,
    invocation: &ScenarioCallbackInvocation,
) -> mlua::Result<Table> {
    let event = lua.create_table()?;
    event.set("scenario_id", invocation.scenario_id().as_str())?;
    event.set("callback", invocation.callback())?;
    event.set("trigger", invocation.trigger().name())?;
    event.set(
        "fired_at",
        unix_timestamp(invocation.fired_at(), "fired_at")?,
    )?;

    match invocation.trigger() {
        ScenarioCallbackTrigger::Timer { delay } => {
            event.set("delay_seconds", delay.as_secs_f64())?;
        }

        ScenarioCallbackTrigger::AbsoluteTime { scheduled_at } => {
            event.set(
                "scheduled_at",
                unix_timestamp(*scheduled_at, "scheduled_at")?,
            )?;
        }

        ScenarioCallbackTrigger::Measurement {
            measurement,
            condition,
        } => {
            if let Some(measurement) = measurement {
                event.set("series", measurement.series.as_str())?;
                event.set("value", measurement.value)?;
                event.set("timestamp", measurement.timestamp)?;
            } else if let Some(series) = condition.primary_series() {
                event.set("series", series)?;
            }
            event.set("condition", condition.kind_name())?;
            event.set("for_seconds", condition.hold.as_secs_f64())?;
            event.set("definition", scenario_condition_definition(lua, condition)?)?;

            match &condition.kind {
                ScenarioConditionKind::Threshold {
                    threshold,
                    hysteresis,
                    ..
                }
                | ScenarioConditionKind::Rate {
                    threshold,
                    hysteresis,
                    ..
                } => {
                    event.set("threshold", *threshold)?;
                    event.set("hysteresis", *hysteresis)?;
                }
                ScenarioConditionKind::Inside {
                    minimum,
                    maximum,
                    hysteresis,
                    ..
                }
                | ScenarioConditionKind::Outside {
                    minimum,
                    maximum,
                    hysteresis,
                    ..
                } => {
                    event.set("min", *minimum)?;
                    event.set("max", *maximum)?;
                    event.set("hysteresis", *hysteresis)?;
                }
                ScenarioConditionKind::Stable {
                    target,
                    tolerance,
                    hysteresis,
                    ..
                } => {
                    event.set("target", *target)?;
                    event.set("tolerance", *tolerance)?;
                    event.set("hysteresis", *hysteresis)?;
                }
                ScenarioConditionKind::Stale { duration, .. } => {
                    event.set("stale_for_seconds", duration.as_secs_f64())?;
                }
                ScenarioConditionKind::All(_) | ScenarioConditionKind::Any(_) => {}
            }
        }

        ScenarioCallbackTrigger::Stage {
            stage,
            previous_stage,
            reason,
            transition_trigger,
        } => {
            event.set("stage", stage.as_str())?;
            event.set(
                "previous_stage",
                previous_stage.as_ref().map(|stage| stage.as_str()),
            )?;
            event.set("reason", reason.as_str())?;
            event.set("transition_trigger", transition_trigger.as_deref())?;
        }

        ScenarioCallbackTrigger::Stop { status, reason } => {
            event.set("status", status.as_str())?;
            event.set("reason", reason.as_str())?;
        }

        ScenarioCallbackTrigger::Error { error } => {
            event.set("status", "failed")?;
            event.set("error", error.as_str())?;
        }
    }

    Ok(event)
}

fn scenario_condition_definition(lua: &Lua, condition: &ScenarioCondition) -> mlua::Result<Table> {
    let definition = lua.create_table()?;
    definition.set("kind", condition.kind_name())?;
    definition.set("for_seconds", condition.hold.as_secs_f64())?;

    match &condition.kind {
        ScenarioConditionKind::Threshold {
            series,
            threshold,
            hysteresis,
            ..
        } => {
            definition.set("series", series.as_str())?;
            definition.set("threshold", *threshold)?;
            definition.set("hysteresis", *hysteresis)?;
        }
        ScenarioConditionKind::Inside {
            series,
            minimum,
            maximum,
            hysteresis,
        }
        | ScenarioConditionKind::Outside {
            series,
            minimum,
            maximum,
            hysteresis,
        } => {
            definition.set("series", series.as_str())?;
            definition.set("min", *minimum)?;
            definition.set("max", *maximum)?;
            definition.set("hysteresis", *hysteresis)?;
        }
        ScenarioConditionKind::Stable {
            series,
            target,
            tolerance,
            hysteresis,
        } => {
            definition.set("series", series.as_str())?;
            definition.set("target", *target)?;
            definition.set("tolerance", *tolerance)?;
            definition.set("hysteresis", *hysteresis)?;
        }
        ScenarioConditionKind::Rate {
            series,
            threshold,
            window,
            hysteresis,
            ..
        } => {
            definition.set("series", series.as_str())?;
            definition.set("threshold", *threshold)?;
            definition.set("window_seconds", window.as_secs_f64())?;
            definition.set("hysteresis", *hysteresis)?;
        }
        ScenarioConditionKind::Stale { series, duration } => {
            definition.set("series", series.as_str())?;
            definition.set("stale_for_seconds", duration.as_secs_f64())?;
        }
        ScenarioConditionKind::All(children) | ScenarioConditionKind::Any(children) => {
            let definitions = lua.create_table()?;
            for (index, child) in children.iter().enumerate() {
                definitions.set(index + 1, scenario_condition_definition(lua, child)?)?;
            }
            definition.set("conditions", definitions)?;
        }
    }

    Ok(definition)
}

fn unix_timestamp(time: SystemTime, field: &str) -> mlua::Result<f64> {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .map_err(|_| {
            runtime_error(format!(
                "Scenario callback event field '{field}' is before the Unix epoch",
            ))
        })
}

pub(crate) fn install(
    lua: &Lua,
    app: &Table,
    event_sender: Sender<LuaApplicationEvent>,
) -> mlua::Result<()> {
    let registry = lua.create_table()?;
    let metadata = Rc::new(RefCell::new(ScriptMetadataRegistry::default()));

    let set_control_event_sender = event_sender.clone();
    let set_control_metadata = Rc::clone(&metadata);

    let set_control = lua.create_function(
        move |_, (script_id, panel_id, control_id, value): (String, String, String, Value)| {
            validate_identifier("application script", &script_id)?;

            validate_identifier("control panel", &panel_id)?;

            validate_identifier("control", &control_id)?;

            let kind =
                set_control_metadata
                    .borrow()
                    .control_kind(&script_id, &panel_id, &control_id)?;

            let value = parse_control_value(kind, &control_id, value)?;

            send_event(
                &set_control_event_sender,
                LuaApplicationEvent::ControlValueChanged {
                    script_id,
                    panel_id,
                    control_id,
                    value,
                },
            )
        },
    )?;

    app.set("set_control", set_control)?;

    let set_enabled_event_sender = event_sender.clone();
    let set_enabled_metadata = Rc::clone(&metadata);

    let set_control_enabled = lua.create_function(
        move |_,
              (script_id, panel_id, control_id, enabled, reason): (
            String,
            String,
            String,
            bool,
            Option<String>,
        )| {
            validate_identifier("application script", &script_id)?;
            validate_identifier("control panel", &panel_id)?;
            validate_identifier("control", &control_id)?;

            set_enabled_metadata
                .borrow()
                .control_kind(&script_id, &panel_id, &control_id)?;

            let reason = if enabled {
                None
            } else {
                reason.filter(|reason| !reason.trim().is_empty())
            };

            send_event(
                &set_enabled_event_sender,
                LuaApplicationEvent::ControlEnabledChanged {
                    script_id,
                    panel_id,
                    control_id,
                    enabled,
                    reason,
                },
            )
        },
    )?;

    app.set("set_control_enabled", set_control_enabled)?;

    lua.set_named_registry_value(SCRIPT_REGISTRY_KEY, registry)?;

    let register_event_sender = event_sender.clone();
    let register_metadata = Rc::clone(&metadata);

    let register = lua.create_function(move |lua, script: Table| {
        let registration = parse_script_registration(&script)?;

        let registry: Table = lua.named_registry_value(SCRIPT_REGISTRY_KEY)?;

        registry.set(registration.id.clone(), script)?;
        register_metadata
            .borrow_mut()
            .register(&registration.id, &registration.panels);

        send_event(
            &register_event_sender,
            LuaApplicationEvent::ScriptRegistered {
                script_id: registration.id,
                panels: registration.panels,
            },
        )
    })?;

    app.set("register_script", register)?;

    let unregister_metadata = Rc::clone(&metadata);
    let unregister = lua.create_function(move |lua, script_id: String| {
        validate_identifier("application script", &script_id)?;

        let registry: Table = lua.named_registry_value(SCRIPT_REGISTRY_KEY)?;

        registry.set(script_id.clone(), Value::Nil)?;
        unregister_metadata.borrow_mut().unregister(&script_id);

        send_event(
            &event_sender,
            LuaApplicationEvent::ScriptUnregistered { script_id },
        )
    })?;

    app.set("unregister_script", unregister)?;

    Ok(())
}

struct ScriptRegistration {
    id: String,
    panels: Vec<ControlPanelDefinition>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegisteredControlKind {
    Readout,
    Number,
    Toggle,
    Button,
}

impl RegisteredControlKind {
    const fn key(self) -> &'static str {
        match self {
            Self::Readout => "readout",
            Self::Number => "number",
            Self::Toggle => "toggle",
            Self::Button => "button",
        }
    }
}

#[derive(Default)]
struct ScriptMetadataRegistry {
    scripts: HashMap<String, HashMap<String, HashMap<String, RegisteredControlKind>>>,
}

impl ScriptMetadataRegistry {
    fn register(&mut self, script_id: &str, panels: &[ControlPanelDefinition]) {
        let panels = panels
            .iter()
            .map(|panel| {
                let controls = panel
                    .controls()
                    .iter()
                    .map(|control| {
                        let kind = match control {
                            ControlDefinition::Readout { .. } => RegisteredControlKind::Readout,
                            ControlDefinition::Number { .. } => RegisteredControlKind::Number,
                            ControlDefinition::Toggle { .. } => RegisteredControlKind::Toggle,
                            ControlDefinition::Button { .. } => RegisteredControlKind::Button,
                        };

                        (control.id().to_owned(), kind)
                    })
                    .collect();

                (panel.id().to_owned(), controls)
            })
            .collect();

        self.scripts.insert(script_id.to_owned(), panels);
    }

    fn unregister(&mut self, script_id: &str) {
        self.scripts.remove(script_id);
    }

    fn control_kind(
        &self,
        script_id: &str,
        panel_id: &str,
        control_id: &str,
    ) -> mlua::Result<RegisteredControlKind> {
        let panels = self.scripts.get(script_id).ok_or_else(|| {
            runtime_error(format!(
                "Application script '{script_id}' is not registered"
            ))
        })?;
        let controls = panels.get(panel_id).ok_or_else(|| {
            runtime_error(format!(
                "Control panel '{panel_id}' was not found in application script '{script_id}'",
            ))
        })?;

        controls.get(control_id).copied().ok_or_else(|| {
            runtime_error(format!(
                "Control '{control_id}' was not found in panel '{panel_id}' of application script '{script_id}'",
            ))
        })
    }
}

fn parse_script_registration(script: &Table) -> mlua::Result<ScriptRegistration> {
    let id = required_string(script, "id", "application script")?;

    validate_identifier("application script", &id)?;

    let panels = match script.get::<Option<Table>>("panels")? {
        Some(panels) => parse_control_panels(&panels, &id)?,

        None => Vec::new(),
    };

    validate_callbacks(script, &id, &panels)?;

    Ok(ScriptRegistration { id, panels })
}

fn parse_control_panels(
    panels: &Table,
    script_id: &str,
) -> mlua::Result<Vec<ControlPanelDefinition>> {
    let length = validate_array(
        panels,
        &format!("panels of application script '{script_id}'",),
    )?;

    let mut definitions = Vec::with_capacity(length);
    let mut panel_ids = HashSet::new();

    for index in 1..=length {
        let panel = panels.raw_get::<Table>(index)?;

        let definition = parse_control_panel(&panel, script_id, index)?;

        if !panel_ids.insert(definition.id().to_owned()) {
            return Err(runtime_error(format!(
                "Application script '{script_id}' \
                 contains duplicate panel id '{}'",
                definition.id(),
            )));
        }

        definitions.push(definition);
    }

    Ok(definitions)
}

fn parse_control_panel(
    panel: &Table,
    script_id: &str,
    index: usize,
) -> mlua::Result<ControlPanelDefinition> {
    let context = format!(
        "control panel #{index} of \
         application script '{script_id}'",
    );

    validate_keys(panel, &context, &["id", "title", "controls"])?;

    let id = required_string(panel, "id", &context)?;

    let title = required_string(
        panel,
        "title",
        &format!(
            "control panel '{id}' of \
             application script '{script_id}'",
        ),
    )?;

    let controls = panel.get::<Option<Table>>("controls")?.ok_or_else(|| {
        runtime_error(format!(
            "Control panel '{id}' of \
                 application script '{script_id}' \
                 must contain 'controls'",
        ))
    })?;

    let controls = parse_controls(&controls, script_id, &id)?;

    ControlPanelDefinition::new(id, title, controls)
        .map_err(|error| runtime_error(error.to_string()))
}

fn parse_controls(
    controls: &Table,
    script_id: &str,
    panel_id: &str,
) -> mlua::Result<Vec<ControlDefinition>> {
    let length = validate_array(
        controls,
        &format!(
            "controls of panel '{panel_id}' \
             in application script '{script_id}'",
        ),
    )?;

    let mut definitions = Vec::with_capacity(length);

    for index in 1..=length {
        let control = controls.raw_get::<Table>(index)?;

        definitions.push(parse_control(&control, script_id, panel_id, index)?);
    }

    Ok(definitions)
}

fn parse_control(
    control: &Table,
    script_id: &str,
    panel_id: &str,
    index: usize,
) -> mlua::Result<ControlDefinition> {
    let context = format!(
        "control #{index} of panel '{panel_id}' \
         in application script '{script_id}'",
    );

    let kind = required_string(control, "kind", &context)?;

    let id = required_string(control, "id", &context)?;

    let control_context = format!(
        "{kind} control '{id}' of panel \
         '{panel_id}' in application script \
         '{script_id}'",
    );

    let label = required_string(control, "label", &control_context)?;

    match kind.as_str() {
        "readout" => {
            validate_keys(
                control,
                &control_context,
                &["kind", "id", "label", "initial"],
            )?;

            let initial_text = control
                .get::<Option<String>>("initial")?
                .unwrap_or_else(|| "—".to_owned());

            Ok(ControlDefinition::Readout {
                id,
                label,
                initial_text,
            })
        }

        "number" => {
            validate_keys(
                control,
                &control_context,
                &[
                    "kind",
                    "id",
                    "label",
                    "initial",
                    "min",
                    "max",
                    "step",
                    "on_change",
                ],
            )?;

            let initial_value = control.get::<Option<f64>>("initial")?.unwrap_or(0.0);

            let minimum = control.get::<Option<f64>>("min")?;

            let maximum = control.get::<Option<f64>>("max")?;

            let step = control.get::<Option<f64>>("step")?.unwrap_or(1.0);

            let on_change = required_string(control, "on_change", &control_context)?;

            Ok(ControlDefinition::Number {
                id,
                label,
                initial_value,
                minimum,
                maximum,
                step,
                on_change,
            })
        }

        "toggle" => {
            validate_keys(
                control,
                &control_context,
                &["kind", "id", "label", "initial", "on_change"],
            )?;

            let initial_value = control.get::<Option<bool>>("initial")?.unwrap_or(false);

            let on_change = required_string(control, "on_change", &control_context)?;

            Ok(ControlDefinition::Toggle {
                id,
                label,
                initial_value,
                on_change,
            })
        }

        "button" => {
            validate_keys(
                control,
                &control_context,
                &["kind", "id", "label", "on_click"],
            )?;

            let on_click = required_string(control, "on_click", &control_context)?;

            Ok(ControlDefinition::Button {
                id,
                label,
                on_click,
            })
        }

        _ => Err(runtime_error(format!(
            "Unknown control kind '{kind}' for \
             control '{id}' in application \
             script '{script_id}'",
        ))),
    }
}

fn parse_control_value(
    kind: RegisteredControlKind,
    control_id: &str,
    value: Value,
) -> mlua::Result<LuaControlValue> {
    let actual_type = value.type_name();

    let value = match (kind, value) {
        (RegisteredControlKind::Readout, Value::String(value)) => {
            LuaControlValue::Text(value.to_str()?.to_string())
        }

        (RegisteredControlKind::Number, Value::Integer(value)) => {
            LuaControlValue::Number(value as f64)
        }

        (RegisteredControlKind::Number, Value::Number(value)) if value.is_finite() => {
            LuaControlValue::Number(value)
        }

        (RegisteredControlKind::Toggle, Value::Boolean(value)) => LuaControlValue::Boolean(value),

        (RegisteredControlKind::Button, _) => {
            return Err(runtime_error(format!(
                "Button control '{control_id}' \
                 cannot receive a value",
            )));
        }

        _ => {
            return Err(runtime_error(format!(
                "Cannot assign Lua {actual_type} \
                 to {} control '{control_id}'",
                kind.key(),
            )));
        }
    };

    Ok(value)
}

fn validate_callbacks(
    script: &Table,
    script_id: &str,
    panels: &[ControlPanelDefinition],
) -> mlua::Result<()> {
    for panel in panels {
        for control in panel.controls() {
            let callback = match control {
                ControlDefinition::Readout { .. } => {
                    continue;
                }

                ControlDefinition::Number { on_change, .. }
                | ControlDefinition::Toggle { on_change, .. } => on_change,

                ControlDefinition::Button { on_click, .. } => on_click,
            };

            match script.get::<Value>(callback.as_str())? {
                Value::Function(_) => {}

                _ => {
                    return Err(runtime_error(format!(
                        "Application script \
                         '{script_id}' callback \
                         '{callback}' must be a function",
                    )));
                }
            }
        }
    }

    Ok(())
}

fn validate_array(table: &Table, context: &str) -> mlua::Result<usize> {
    let length = table.raw_len();
    let mut entry_count = 0;

    for pair in table.clone().pairs::<Value, Value>() {
        let (key, _) = pair?;

        let Value::Integer(index) = key else {
            return Err(runtime_error(format!("{context} must be an array",)));
        };

        let valid_index = usize::try_from(index)
            .ok()
            .is_some_and(|index| index >= 1 && index <= length);

        if !valid_index {
            return Err(runtime_error(format!(
                "{context} must be a continuous array",
            )));
        }

        entry_count += 1;
    }

    if entry_count != length {
        return Err(runtime_error(format!(
            "{context} must be a continuous array",
        )));
    }

    Ok(length)
}

fn validate_keys(table: &Table, context: &str, allowed_keys: &[&str]) -> mlua::Result<()> {
    for pair in table.clone().pairs::<String, Value>() {
        let (key, _) = pair?;

        if !allowed_keys.contains(&key.as_str()) {
            return Err(runtime_error(format!(
                "Unknown option '{key}' in {context}",
            )));
        }
    }

    Ok(())
}

fn required_string(table: &Table, key: &str, context: &str) -> mlua::Result<String> {
    table
        .get::<Option<String>>(key)?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            runtime_error(format!(
                "{context} must contain non-empty \
                 string '{key}'",
            ))
        })
}

fn validate_identifier(kind: &str, identifier: &str) -> mlua::Result<()> {
    let mut characters = identifier.chars();

    let valid_first = characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_');

    let valid_remaining =
        characters.all(|character| character.is_ascii_alphanumeric() || character == '_');

    if !valid_first || !valid_remaining {
        return Err(runtime_error(format!(
            "Invalid {kind} id '{identifier}': \
             use an ASCII letter or underscore \
             first, followed by letters, digits \
             or underscores",
        )));
    }

    Ok(())
}

fn send_event(
    sender: &Sender<LuaApplicationEvent>,
    event: LuaApplicationEvent,
) -> mlua::Result<()> {
    sender.send(event).map_err(|_| {
        runtime_error(
            "Lua application event channel \
                 is disconnected",
        )
    })
}

fn runtime_error(message: impl Into<String>) -> mlua::Error {
    mlua::Error::RuntimeError(message.into())
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use crossbeam_channel::unbounded;
    use mlua::{Lua, Table, Value};

    use super::{
        LuaApplicationEvent, LuaControlArgument, LuaControlInvocation, LuaControlValue,
        SCRIPT_REGISTRY_KEY, install, invoke_control_callback, invoke_scenario_callback,
    };
    use crate::scenario::{
        ScenarioCallbackInvocation, ScenarioCallbackTrigger, ScenarioCondition,
        ScenarioConditionKind, ScenarioId, ScenarioMeasurementContext, ScenarioStageName,
        ThresholdDirection,
    };

    #[test]
    fn registers_application_script() {
        let lua = Lua::new();
        let app = lua.create_table().unwrap();

        let (event_sender, event_receiver) = unbounded();
        install(&lua, &app, event_sender).unwrap();

        lua.globals().set("app", app).unwrap();

        lua.load(
            r#"
                local script = {
                    id = "demo",

                    panels = {
                        {
                            id = "controls",
                            title = "Demo",

                            controls = {
                                {
                                    kind = "number",
                                    id = "value",
                                    label = "Value",
                                    initial = 10.0,
                                    min = 0.0,
                                    max = 100.0,
                                    step = 1.0,
                                    on_change = "set_value",
                                },

                                {
                                    kind = "button",
                                    id = "start",
                                    label = "Start",
                                    on_click = "run",
                                },
                            },
                        },
                    },
                }

                function script.set_value(value)
                    script.value = value
                end

                function script.run()
                    script.running = true
                end

                app.register_script(script)
            "#,
        )
        .exec()
        .unwrap();

        let registry: Table = lua.named_registry_value(SCRIPT_REGISTRY_KEY).unwrap();

        let script: Table = registry.get("demo").unwrap();

        assert_eq!(script.get::<String>("id").unwrap(), "demo",);

        let event = event_receiver.recv().unwrap();

        let LuaApplicationEvent::ScriptRegistered { script_id, panels } = event else {
            panic!("expected script registration event");
        };

        assert_eq!(script_id, "demo");
        assert_eq!(panels.len(), 1);
        assert_eq!(panels[0].id(), "controls");
    }

    #[test]
    fn rejects_missing_callback() {
        let lua = Lua::new();
        let app = lua.create_table().unwrap();

        let (event_sender, _event_receiver) = unbounded();
        install(&lua, &app, event_sender).unwrap();

        lua.globals().set("app", app).unwrap();

        let error = lua
            .load(
                r#"
                    app.register_script({
                        id = "demo",

                        panels = {
                            {
                                id = "controls",
                                title = "Demo",

                                controls = {
                                    {
                                        kind = "button",
                                        id = "start",
                                        label = "Start",
                                        on_click = "run",
                                    },
                                },
                            },
                        },
                    })
                "#,
            )
            .exec()
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("callback 'run' must be a function",),
        );
    }

    #[test]
    fn unregisters_application_script() {
        let lua = Lua::new();
        let app = lua.create_table().unwrap();

        let (event_sender, event_receiver) = unbounded();
        install(&lua, &app, event_sender).unwrap();

        lua.globals().set("app", app).unwrap();

        lua.load(
            r#"
                local script = {
                    id = "demo",
                }

                app.register_script(script)
                app.unregister_script("demo")
            "#,
        )
        .exec()
        .unwrap();

        let _registration = event_receiver.recv().unwrap();

        assert_eq!(
            event_receiver.recv().unwrap(),
            LuaApplicationEvent::ScriptUnregistered {
                script_id: "demo".to_owned(),
            },
        );

        let registry: Table = lua.named_registry_value(SCRIPT_REGISTRY_KEY).unwrap();

        let value: Value = registry.get("demo").unwrap();

        assert_eq!(value, Value::Nil);
    }

    #[test]
    fn invokes_registered_control_callback() {
        let lua = Lua::new();
        let app = lua.create_table().unwrap();

        let (event_sender, event_receiver) = unbounded();

        install(&lua, &app, event_sender).unwrap();

        lua.globals().set("app", app).unwrap();

        lua.load(
            r#"
                local script = {
                    id = "demo",
                }

                function script.set_value(value)
                    script.value = value
                end

                app.register_script(script)
            "#,
        )
        .exec()
        .unwrap();

        let _registration = event_receiver.recv().unwrap();

        let invocation = LuaControlInvocation::new(
            "demo",
            "controls",
            "value",
            "set_value",
            Some(LuaControlArgument::Number(42.0)),
        );

        invoke_control_callback(&lua, &invocation).unwrap();

        let registry: Table = lua.named_registry_value(SCRIPT_REGISTRY_KEY).unwrap();

        let script: Table = registry.get("demo").unwrap();

        assert_eq!(script.get::<f64>("value").unwrap(), 42.0);
    }

    #[test]
    fn invokes_scenario_callback_from_registered_script() {
        let lua = Lua::new();
        let app = lua.create_table().unwrap();
        let (event_sender, _event_receiver) = unbounded();
        install(&lua, &app, event_sender).unwrap();
        lua.globals().set("app", app).unwrap();

        lua.load(
            r#"
                local script = { id = "demo" }
                function script.advance(event)
                    scenario_event = event
                end
                app.register_script(script)
            "#,
        )
        .exec()
        .unwrap();

        let invocation = ScenarioCallbackInvocation::new(
            ScenarioId::new("heat_cycle").unwrap(),
            "advance".to_owned(),
            SystemTime::UNIX_EPOCH + Duration::from_secs_f64(20.25),
            ScenarioCallbackTrigger::Timer {
                delay: Duration::from_secs_f64(5.5),
            },
        );

        invoke_scenario_callback(&lua, &invocation).unwrap();

        let event = lua.globals().get::<Table>("scenario_event").unwrap();
        assert_eq!(event.get::<String>("scenario_id").unwrap(), "heat_cycle");
        assert_eq!(event.get::<String>("callback").unwrap(), "advance");
        assert_eq!(event.get::<String>("trigger").unwrap(), "timer");
        assert_eq!(event.get::<f64>("fired_at").unwrap(), 20.25);
        assert_eq!(event.get::<f64>("delay_seconds").unwrap(), 5.5);
    }

    #[test]
    fn passes_absolute_time_context_to_global_scenario_callback() {
        let lua = Lua::new();
        lua.load("function capture(event) scenario_event = event end")
            .exec()
            .unwrap();

        let invocation = ScenarioCallbackInvocation::new(
            ScenarioId::new("scheduled_cycle").unwrap(),
            "capture".to_owned(),
            SystemTime::UNIX_EPOCH + Duration::from_secs_f64(30.5),
            ScenarioCallbackTrigger::AbsoluteTime {
                scheduled_at: SystemTime::UNIX_EPOCH + Duration::from_secs_f64(25.75),
            },
        );

        invoke_scenario_callback(&lua, &invocation).unwrap();

        let event = lua.globals().get::<Table>("scenario_event").unwrap();
        assert_eq!(
            event.get::<String>("scenario_id").unwrap(),
            "scheduled_cycle"
        );
        assert_eq!(event.get::<String>("callback").unwrap(), "capture");
        assert_eq!(event.get::<String>("trigger").unwrap(), "absolute_time");
        assert_eq!(event.get::<f64>("fired_at").unwrap(), 30.5);
        assert_eq!(event.get::<f64>("scheduled_at").unwrap(), 25.75);
    }

    #[test]
    fn passes_measurement_context_to_global_scenario_callback() {
        let lua = Lua::new();
        lua.load("function capture(event) scenario_event = event end")
            .exec()
            .unwrap();

        let invocation = ScenarioCallbackInvocation::new(
            ScenarioId::new("heat_cycle").unwrap(),
            "capture".to_owned(),
            SystemTime::UNIX_EPOCH + Duration::from_secs_f64(50.0),
            ScenarioCallbackTrigger::Measurement {
                measurement: Some(ScenarioMeasurementContext {
                    series: "temperature".to_owned(),
                    value: 151.25,
                    timestamp: 49.5,
                }),
                condition: ScenarioCondition {
                    kind: ScenarioConditionKind::Threshold {
                        series: "temperature".to_owned(),
                        direction: ThresholdDirection::Above,
                        threshold: 150.0,
                        hysteresis: 2.0,
                    },
                    hold: Duration::from_secs(5),
                },
            },
        );

        invoke_scenario_callback(&lua, &invocation).unwrap();

        let event = lua.globals().get::<Table>("scenario_event").unwrap();
        assert_eq!(event.get::<String>("trigger").unwrap(), "measurement");
        assert_eq!(event.get::<String>("series").unwrap(), "temperature");
        assert_eq!(event.get::<f64>("value").unwrap(), 151.25);
        assert_eq!(event.get::<f64>("timestamp").unwrap(), 49.5);
        assert_eq!(event.get::<String>("condition").unwrap(), "above");
        assert_eq!(event.get::<f64>("threshold").unwrap(), 150.0);
        assert_eq!(event.get::<f64>("for_seconds").unwrap(), 5.0);
        assert_eq!(event.get::<f64>("hysteresis").unwrap(), 2.0);
    }

    #[test]
    fn passes_stage_context_to_global_scenario_callback() {
        let lua = Lua::new();
        lua.load("function capture(event) scenario_event = event end")
            .exec()
            .unwrap();

        let invocation = ScenarioCallbackInvocation::new(
            ScenarioId::new("heat_cycle").unwrap(),
            "capture".to_owned(),
            SystemTime::UNIX_EPOCH + Duration::from_secs(60),
            ScenarioCallbackTrigger::Stage {
                stage: ScenarioStageName::new("holding").unwrap(),
                previous_stage: Some(ScenarioStageName::new("heating").unwrap()),
                reason: "Target temperature reached".to_owned(),
                transition_trigger: Some("measurement".to_owned()),
            },
        );

        invoke_scenario_callback(&lua, &invocation).unwrap();

        let event = lua.globals().get::<Table>("scenario_event").unwrap();
        assert_eq!(event.get::<String>("trigger").unwrap(), "stage");
        assert_eq!(event.get::<String>("stage").unwrap(), "holding");
        assert_eq!(event.get::<String>("previous_stage").unwrap(), "heating");
        assert_eq!(
            event.get::<String>("reason").unwrap(),
            "Target temperature reached"
        );
        assert_eq!(
            event.get::<String>("transition_trigger").unwrap(),
            "measurement"
        );
    }

    #[test]
    fn passes_stale_condition_context_without_synthetic_measurement() {
        let lua = Lua::new();
        lua.load("function capture(event) scenario_event = event end")
            .exec()
            .unwrap();
        let invocation = ScenarioCallbackInvocation::new(
            ScenarioId::new("monitor").unwrap(),
            "capture".to_owned(),
            SystemTime::UNIX_EPOCH + Duration::from_secs(60),
            ScenarioCallbackTrigger::Measurement {
                measurement: None,
                condition: ScenarioCondition {
                    kind: ScenarioConditionKind::Stale {
                        series: "temperature".to_owned(),
                        duration: Duration::from_secs(5),
                    },
                    hold: Duration::ZERO,
                },
            },
        );

        invoke_scenario_callback(&lua, &invocation).unwrap();

        let event = lua.globals().get::<Table>("scenario_event").unwrap();
        assert_eq!(event.get::<String>("trigger").unwrap(), "measurement");
        assert_eq!(event.get::<String>("condition").unwrap(), "stale");
        assert_eq!(event.get::<String>("series").unwrap(), "temperature");
        assert_eq!(event.get::<f64>("stale_for_seconds").unwrap(), 5.0);
        assert!(event.get::<Option<f64>>("value").unwrap().is_none());
        assert!(event.get::<Option<f64>>("timestamp").unwrap().is_none());
    }

    #[test]
    fn rejects_scenario_context_timestamp_before_unix_epoch() {
        let lua = Lua::new();
        lua.load("function capture(_) end").exec().unwrap();

        let invocation = ScenarioCallbackInvocation::new(
            ScenarioId::new("heat_cycle").unwrap(),
            "capture".to_owned(),
            SystemTime::UNIX_EPOCH - Duration::from_secs(1),
            ScenarioCallbackTrigger::Timer {
                delay: Duration::ZERO,
            },
        );

        let error = invoke_scenario_callback(&lua, &invocation).unwrap_err();
        assert!(error.to_string().contains("fired_at"));
    }

    #[test]
    fn publishes_control_value_change() {
        let lua = Lua::new();
        let app = lua.create_table().unwrap();

        let (event_sender, event_receiver) = unbounded();

        install(&lua, &app, event_sender).unwrap();

        lua.globals().set("app", app).unwrap();

        lua.load(
            r#"
                local script = {
                    id = "demo",

                    panels = {
                        {
                            id = "controls",
                            title = "Demo",

                            controls = {
                                {
                                    kind = "readout",
                                    id = "status",
                                    label = "Status",
                                    initial = "Waiting",
                                },
                            },
                        },
                    },
                }

                app.register_script(script)

                app.set_control(
                    "demo",
                    "controls",
                    "status",
                    "Running"
                )

                app.set_control_enabled(
                    "demo",
                    "controls",
                    "status",
                    false,
                    "Waiting for acquisition"
                )
            "#,
        )
        .exec()
        .unwrap();

        let _registration = event_receiver.recv().unwrap();

        assert_eq!(
            event_receiver.recv().unwrap(),
            LuaApplicationEvent::ControlValueChanged {
                script_id: "demo".to_owned(),
                panel_id: "controls".to_owned(),
                control_id: "status".to_owned(),
                value: LuaControlValue::Text("Running".to_owned(),),
            },
        );

        assert_eq!(
            event_receiver.recv().unwrap(),
            LuaApplicationEvent::ControlEnabledChanged {
                script_id: "demo".to_owned(),
                panel_id: "controls".to_owned(),
                control_id: "status".to_owned(),
                enabled: false,
                reason: Some("Waiting for acquisition".to_owned()),
            },
        );
    }

    #[test]
    fn control_updates_use_validated_registration_metadata() {
        let lua = Lua::new();
        let app = lua.create_table().unwrap();
        let (event_sender, event_receiver) = unbounded();
        install(&lua, &app, event_sender).unwrap();
        lua.globals().set("app", app).unwrap();

        lua.load(
            r#"
                local script = {
                    id = "demo",
                    panels = {{
                        id = "controls",
                        title = "Demo",
                        controls = {{
                            kind = "number",
                            id = "value",
                            label = "Value",
                            on_change = "set_value",
                        }},
                    }},
                }

                function script.set_value(_) end

                app.register_script(script)
                script.panels[1].controls[1].kind = "button"
                app.set_control("demo", "controls", "value", 42.0)
            "#,
        )
        .exec()
        .unwrap();

        let _registration = event_receiver.recv().unwrap();
        assert_eq!(
            event_receiver.recv().unwrap(),
            LuaApplicationEvent::ControlValueChanged {
                script_id: "demo".to_owned(),
                panel_id: "controls".to_owned(),
                control_id: "value".to_owned(),
                value: LuaControlValue::Number(42.0),
            },
        );
    }
}
