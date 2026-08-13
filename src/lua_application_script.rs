use std::collections::HashSet;

use crossbeam_channel::Sender;
use mlua::{Lua, Table, Value};

use crate::control_panel::{ControlDefinition, ControlPanelDefinition};

const SCRIPT_REGISTRY_KEY: &str = "com_port_reader.application_scripts";

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LuaApplicationEvent {
    ScriptRegistered {
        script_id: String,
        panels: Vec<ControlPanelDefinition>,
    },

    ScriptUnregistered {
        script_id: String,
    },
}

pub(crate) fn install(
    lua: &Lua,
    app: &Table,
    event_sender: Sender<LuaApplicationEvent>,
) -> mlua::Result<()> {
    let registry = lua.create_table()?;

    lua.set_named_registry_value(SCRIPT_REGISTRY_KEY, registry)?;

    let register_event_sender = event_sender.clone();

    let register = lua.create_function(move |lua, script: Table| {
        let registration = parse_script_registration(&script)?;

        let registry: Table = lua.named_registry_value(SCRIPT_REGISTRY_KEY)?;

        registry.set(registration.id.clone(), script)?;

        send_event(
            &register_event_sender,
            LuaApplicationEvent::ScriptRegistered {
                script_id: registration.id,
                panels: registration.panels,
            },
        )
    })?;

    app.set("register_script", register)?;

    let unregister = lua.create_function(move |lua, script_id: String| {
        validate_identifier("application script", &script_id)?;

        let registry: Table = lua.named_registry_value(SCRIPT_REGISTRY_KEY)?;

        registry.set(script_id.clone(), Value::Nil)?;

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
    use crossbeam_channel::unbounded;
    use mlua::{Lua, Table, Value};

    use super::{LuaApplicationEvent, SCRIPT_REGISTRY_KEY, install};

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

        let (event_sender, event_receiver) = unbounded();
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
}
