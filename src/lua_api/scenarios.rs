use std::time::{Duration, SystemTime};

use crossbeam_channel::Sender;
use mlua::{Lua, Table, UserData, UserDataMethods, Value};

use super::send_application_command;
use crate::{
    scenario::{
        ScenarioCommand, ScenarioCondition, ScenarioId, ScenarioRaceAlternative,
        ScenarioRaceTrigger, ThresholdDirection,
    },
    user_command::UserCommand,
};

pub(super) fn register_scenario(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
) -> mlua::Result<()> {
    let function = lua.create_function(move |lua, options: Table| {
        validate_keys(&options, "scenario", &["id"])?;

        let id = required_string(&options, "id", "scenario")?;
        let id =
            ScenarioId::new(id).map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

        send_application_command(
            &command_sender,
            ScenarioCommand::Register { id: id.clone() }.into(),
        )?;

        lua.create_userdata(LuaScenarioHandle {
            id,
            command_sender: command_sender.clone(),
        })
    })?;

    app.set("scenario", function)
}

struct LuaScenarioHandle {
    id: ScenarioId,
    command_sender: Sender<UserCommand>,
}

impl UserData for LuaScenarioHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "after",
            |_, scenario, (delay_seconds, callback): (f64, String)| {
                let delay = parse_duration(delay_seconds, "Scenario delay")?;
                let callback = validate_callback(callback)?;

                send_application_command(
                    &scenario.command_sender,
                    ScenarioCommand::After {
                        id: scenario.id.clone(),
                        delay,
                        callback,
                    }
                    .into(),
                )
            },
        );

        methods.add_method(
            "at",
            |_, scenario, (unix_timestamp, callback): (f64, String)| {
                let deadline = parse_unix_timestamp(unix_timestamp)?;
                let callback = validate_callback(callback)?;

                send_application_command(
                    &scenario.command_sender,
                    ScenarioCommand::At {
                        id: scenario.id.clone(),
                        deadline,
                        callback,
                    }
                    .into(),
                )
            },
        );

        methods.add_method(
            "when",
            |_, scenario, (options, callback): (Table, String)| {
                let condition = parse_condition(&options)?;
                let callback = validate_callback(callback)?;

                send_application_command(
                    &scenario.command_sender,
                    ScenarioCommand::When {
                        id: scenario.id.clone(),
                        condition,
                        callback,
                    }
                    .into(),
                )
            },
        );

        methods.add_method("race", |_, scenario, alternatives: Table| {
            let alternatives = parse_race(&alternatives)?;

            send_application_command(
                &scenario.command_sender,
                ScenarioCommand::Race {
                    id: scenario.id.clone(),
                    alternatives,
                }
                .into(),
            )
        });

        methods.add_method("cancel", |_, scenario, ()| {
            send_application_command(
                &scenario.command_sender,
                ScenarioCommand::Cancel {
                    id: scenario.id.clone(),
                }
                .into(),
            )
        });

        methods.add_method("id", |_, scenario, ()| Ok(scenario.id.as_str().to_owned()));
    }
}

fn parse_race(options: &Table) -> mlua::Result<Vec<ScenarioRaceAlternative>> {
    let mut indexed_alternatives = Vec::new();

    for pair in options.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Value::Integer(index) = key else {
            return Err(mlua::Error::RuntimeError(
                "Scenario race must be an array without named keys".to_owned(),
            ));
        };
        if index <= 0 {
            return Err(mlua::Error::RuntimeError(
                "Scenario race array indices must start at 1".to_owned(),
            ));
        }
        let Value::Table(alternative) = value else {
            return Err(mlua::Error::RuntimeError(format!(
                "Scenario race alternative #{index} must be a table",
            )));
        };

        indexed_alternatives.push((index, alternative));
    }

    indexed_alternatives.sort_by_key(|(index, _)| *index);

    if indexed_alternatives.is_empty() {
        return Err(mlua::Error::RuntimeError(
            "Scenario race must contain at least one alternative".to_owned(),
        ));
    }

    for (offset, (index, _)) in indexed_alternatives.iter().enumerate() {
        let expected = i64::try_from(offset + 1).expect("race alternative index must fit i64");
        if *index != expected {
            return Err(mlua::Error::RuntimeError(
                "Scenario race alternatives must use consecutive array indices starting at 1"
                    .to_owned(),
            ));
        }
    }

    indexed_alternatives
        .into_iter()
        .map(|(_, alternative)| parse_race_alternative(&alternative))
        .collect()
}

fn parse_race_alternative(options: &Table) -> mlua::Result<ScenarioRaceAlternative> {
    validate_keys(
        options,
        "scenario race alternative",
        &["after", "at", "when", "callback"],
    )?;

    let after = options.get::<Option<f64>>("after")?;
    let at = options.get::<Option<f64>>("at")?;
    let when = options.get::<Option<Table>>("when")?;
    let trigger_count =
        usize::from(after.is_some()) + usize::from(at.is_some()) + usize::from(when.is_some());

    if trigger_count != 1 {
        return Err(mlua::Error::RuntimeError(
            "Scenario race alternative must define exactly one of 'after', 'at' or 'when'"
                .to_owned(),
        ));
    }

    let trigger = if let Some(seconds) = after {
        ScenarioRaceTrigger::After {
            delay: parse_duration(seconds, "Scenario race delay")?,
        }
    } else if let Some(timestamp) = at {
        ScenarioRaceTrigger::At {
            deadline: parse_unix_timestamp(timestamp)?,
        }
    } else {
        ScenarioRaceTrigger::When {
            condition: parse_condition(&when.expect("race trigger count was validated"))?,
        }
    };

    Ok(ScenarioRaceAlternative {
        trigger,
        callback: validate_callback(required_string(
            options,
            "callback",
            "scenario race alternative",
        )?)?,
    })
}

fn parse_condition(options: &Table) -> mlua::Result<ScenarioCondition> {
    validate_keys(
        options,
        "scenario condition",
        &[
            "series",
            "above",
            "below",
            "for_seconds",
            "hysteresis",
            "edge",
        ],
    )?;

    let series = required_string(options, "series", "scenario condition")?;
    let above = options.get::<Option<f64>>("above")?;
    let below = options.get::<Option<f64>>("below")?;

    let (direction, threshold) = match (above, below) {
        (Some(threshold), None) => (ThresholdDirection::Above, threshold),
        (None, Some(threshold)) => (ThresholdDirection::Below, threshold),
        _ => {
            return Err(mlua::Error::RuntimeError(
                "Scenario condition must define exactly one of 'above' or 'below'".to_owned(),
            ));
        }
    };

    if !threshold.is_finite() {
        return Err(mlua::Error::RuntimeError(
            "Scenario threshold must be finite".to_owned(),
        ));
    }

    let hold = parse_duration(
        options.get::<Option<f64>>("for_seconds")?.unwrap_or(0.0),
        "Scenario condition for_seconds",
    )?;
    let hysteresis = options.get::<Option<f64>>("hysteresis")?.unwrap_or(0.0);

    if !hysteresis.is_finite() || hysteresis < 0.0 {
        return Err(mlua::Error::RuntimeError(
            "Scenario condition hysteresis must be finite and non-negative".to_owned(),
        ));
    }

    let edge = options
        .get::<Option<String>>("edge")?
        .unwrap_or_else(|| "rising".to_owned());

    if edge != "rising" {
        return Err(mlua::Error::RuntimeError(format!(
            "Unsupported scenario condition edge '{edge}'; expected 'rising'",
        )));
    }

    Ok(ScenarioCondition {
        series,
        direction,
        threshold,
        hold,
        hysteresis,
    })
}

fn parse_duration(seconds: f64, context: &str) -> mlua::Result<Duration> {
    Duration::try_from_secs_f64(seconds).map_err(|_| {
        mlua::Error::RuntimeError(format!("{context} must be finite and non-negative",))
    })
}

fn parse_unix_timestamp(seconds: f64) -> mlua::Result<SystemTime> {
    let offset = parse_duration(seconds, "Scenario Unix timestamp")?;
    SystemTime::UNIX_EPOCH.checked_add(offset).ok_or_else(|| {
        mlua::Error::RuntimeError(
            "Scenario Unix timestamp is outside the supported range".to_owned(),
        )
    })
}

fn validate_callback(callback: String) -> mlua::Result<String> {
    if callback.trim().is_empty() {
        return Err(mlua::Error::RuntimeError(
            "Scenario callback name cannot be empty".to_owned(),
        ));
    }

    Ok(callback)
}

fn validate_keys(table: &Table, context: &str, allowed: &[&str]) -> mlua::Result<()> {
    for pair in table.clone().pairs::<String, Value>() {
        let (key, _) = pair?;
        if !allowed.contains(&key.as_str()) {
            return Err(mlua::Error::RuntimeError(format!(
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
            mlua::Error::RuntimeError(format!("{context} must contain non-empty string '{key}'",))
        })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use crossbeam_channel::unbounded;
    use mlua::Lua;

    use super::register_scenario;
    use crate::{
        scenario::{ScenarioCommand, ScenarioRaceTrigger, ThresholdDirection},
        user_command::UserCommand,
    };

    #[test]
    fn publishes_scenario_commands() {
        let lua = Lua::new();
        let app = lua.create_table().unwrap();
        let (sender, receiver) = unbounded();
        register_scenario(&lua, &app, sender).unwrap();
        lua.globals().set("app", app).unwrap();

        lua.load(
            r#"
                local scenario = app.scenario({ id = "heat_cycle" })
                scenario:after(1.5, "start_heating")
                scenario:at(1700000000.25, "stop_heating")
                scenario:when({
                    series = "temperature",
                    above = 150.0,
                    for_seconds = 5.0,
                    hysteresis = 2.0,
                    edge = "rising",
                }, "switch_to_hold")
                scenario:race({
                    {
                        when = {
                            series = "temperature",
                            above = 180.0,
                        },
                        callback = "temperature_reached",
                    },
                    {
                        after = 30.0,
                        callback = "temperature_timeout",
                    },
                    {
                        at = 1700000030.25,
                        callback = "absolute_timeout",
                    },
                })
                scenario:cancel()
            "#,
        )
        .exec()
        .unwrap();

        assert!(matches!(
            receiver.try_recv().unwrap(),
            UserCommand::Scenario(ScenarioCommand::Register { id })
                if id.as_str() == "heat_cycle"
        ));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            UserCommand::Scenario(ScenarioCommand::After { id, delay, callback })
                if id.as_str() == "heat_cycle"
                    && delay == Duration::from_millis(1_500)
                    && callback == "start_heating"
        ));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            UserCommand::Scenario(ScenarioCommand::At { id, deadline, callback })
                if id.as_str() == "heat_cycle"
                    && deadline == SystemTime::UNIX_EPOCH
                        + Duration::from_millis(1_700_000_000_250)
                    && callback == "stop_heating"
        ));

        let UserCommand::Scenario(ScenarioCommand::When {
            id,
            condition,
            callback,
        }) = receiver.try_recv().unwrap()
        else {
            panic!("expected scenario condition command");
        };
        assert_eq!(id.as_str(), "heat_cycle");
        assert_eq!(condition.series, "temperature");
        assert_eq!(condition.direction, ThresholdDirection::Above);
        assert_eq!(condition.threshold, 150.0);
        assert_eq!(condition.hold, Duration::from_secs(5));
        assert_eq!(condition.hysteresis, 2.0);
        assert_eq!(callback, "switch_to_hold");

        let UserCommand::Scenario(ScenarioCommand::Race { id, alternatives }) =
            receiver.try_recv().unwrap()
        else {
            panic!("expected scenario race command");
        };
        assert_eq!(id.as_str(), "heat_cycle");
        assert_eq!(alternatives.len(), 3);
        assert_eq!(alternatives[0].callback, "temperature_reached");
        assert!(matches!(
            &alternatives[0].trigger,
            ScenarioRaceTrigger::When { condition }
                if condition.series == "temperature"
                    && condition.direction == ThresholdDirection::Above
                    && condition.threshold == 180.0
        ));
        assert!(matches!(
            alternatives[1].trigger,
            ScenarioRaceTrigger::After { delay } if delay == Duration::from_secs(30)
        ));
        assert!(matches!(
            alternatives[2].trigger,
            ScenarioRaceTrigger::At { deadline }
                if deadline == SystemTime::UNIX_EPOCH
                    + Duration::from_millis(1_700_000_030_250)
        ));

        assert!(matches!(
            receiver.try_recv().unwrap(),
            UserCommand::Scenario(ScenarioCommand::Cancel { id })
                if id.as_str() == "heat_cycle"
        ));
    }

    #[test]
    fn rejects_ambiguous_threshold_condition() {
        let lua = Lua::new();
        let app = lua.create_table().unwrap();
        let (sender, _receiver) = unbounded();
        register_scenario(&lua, &app, sender).unwrap();
        lua.globals().set("app", app).unwrap();

        let error = lua
            .load(
                r#"
                    local scenario = app.scenario({ id = "heat_cycle" })
                    scenario:when({
                        series = "temperature",
                        above = 150.0,
                        below = 100.0,
                    }, "callback")
                "#,
            )
            .exec()
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("exactly one of 'above' or 'below'")
        );
    }

    #[test]
    fn rejects_invalid_absolute_time() {
        let lua = Lua::new();
        let app = lua.create_table().unwrap();
        let (sender, _receiver) = unbounded();
        register_scenario(&lua, &app, sender).unwrap();
        lua.globals().set("app", app).unwrap();

        let error = lua
            .load(
                r#"
                    local scenario = app.scenario({ id = "heat_cycle" })
                    scenario:at(-1.0, "callback")
                "#,
            )
            .exec()
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("must be finite and non-negative")
        );
    }

    #[test]
    fn rejects_empty_scenario_race() {
        let error = execute_invalid_race("scenario:race({})");

        assert!(error.contains("must contain at least one alternative"));
    }

    #[test]
    fn rejects_conflicting_scenario_race_triggers() {
        let error = execute_invalid_race(
            r#"
                scenario:race({
                    { after = 1.0, at = 1700000000.0, callback = "callback" },
                })
            "#,
        );

        assert!(error.contains("exactly one of 'after', 'at' or 'when'"));
    }

    #[test]
    fn rejects_sparse_scenario_race() {
        let error = execute_invalid_race(
            r#"
                scenario:race({
                    [2] = { after = 1.0, callback = "callback" },
                })
            "#,
        );

        assert!(error.contains("consecutive array indices"));
    }

    fn execute_invalid_race(source: &str) -> String {
        let lua = Lua::new();
        let app = lua.create_table().unwrap();
        let (sender, _receiver) = unbounded();
        register_scenario(&lua, &app, sender).unwrap();
        lua.globals().set("app", app).unwrap();
        lua.load("scenario = app.scenario({ id = 'heat_cycle' })")
            .exec()
            .unwrap();

        lua.load(source).exec().unwrap_err().to_string()
    }
}
