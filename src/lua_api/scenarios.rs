use std::{
    collections::HashSet,
    time::{Duration, SystemTime},
};

use crossbeam_channel::Sender;
use mlua::{Lua, Table, UserData, UserDataMethods, Value};

use super::send_application_command;
use crate::{
    scenario::{
        ScenarioCommand, ScenarioCondition, ScenarioConditionKind, ScenarioId,
        ScenarioRaceAlternative, ScenarioRaceTrigger, ScenarioStageDefinition, ScenarioStageName,
        ScenarioStageTransition, ThresholdDirection,
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
            stage_names: HashSet::new(),
            stage_targets: Vec::new(),
            stages_started: false,
            stop_handler_registered: false,
            error_handler_registered: false,
        })
    })?;

    app.set("scenario", function)
}

struct LuaScenarioHandle {
    id: ScenarioId,
    command_sender: Sender<UserCommand>,
    stage_names: HashSet<ScenarioStageName>,
    stage_targets: Vec<(ScenarioStageName, ScenarioStageName)>,
    stages_started: bool,
    stop_handler_registered: bool,
    error_handler_registered: bool,
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

        methods.add_method_mut("stage", |_, scenario, (name, options): (String, Table)| {
            if scenario.stages_started {
                return Err(mlua::Error::RuntimeError(format!(
                    "Scenario '{}' cannot define stages after start",
                    scenario.id.as_str(),
                )));
            }
            let name = ScenarioStageName::new(name)
                .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;
            if scenario.stage_names.contains(&name) {
                return Err(mlua::Error::RuntimeError(format!(
                    "Scenario '{}' contains duplicate stage '{}'",
                    scenario.id.as_str(),
                    name.as_str(),
                )));
            }
            let definition = parse_stage_definition(&options, &name)?;
            let targets = definition
                .transitions
                .iter()
                .map(|transition| (name.clone(), transition.next.clone()))
                .collect::<Vec<_>>();

            send_application_command(
                &scenario.command_sender,
                ScenarioCommand::DefineStage {
                    id: scenario.id.clone(),
                    name: name.clone(),
                    definition,
                }
                .into(),
            )?;

            scenario.stage_names.insert(name);
            scenario.stage_targets.extend(targets);
            Ok(())
        });

        methods.add_method_mut("start", |_, scenario, name: String| {
            if scenario.stages_started {
                return Err(mlua::Error::RuntimeError(format!(
                    "Scenario '{}' stages have already been started",
                    scenario.id.as_str(),
                )));
            }
            let stage = ScenarioStageName::new(name)
                .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;
            if !scenario.stage_names.contains(&stage) {
                return Err(mlua::Error::RuntimeError(format!(
                    "Scenario '{}' start stage '{}' is not defined",
                    scenario.id.as_str(),
                    stage.as_str(),
                )));
            }
            if let Some((source, target)) = scenario
                .stage_targets
                .iter()
                .find(|(_, target)| !scenario.stage_names.contains(target))
            {
                return Err(mlua::Error::RuntimeError(format!(
                    "Scenario '{}' stage '{}' has a transition to unknown stage '{}'",
                    scenario.id.as_str(),
                    source.as_str(),
                    target.as_str(),
                )));
            }

            send_application_command(
                &scenario.command_sender,
                ScenarioCommand::Start {
                    id: scenario.id.clone(),
                    stage,
                }
                .into(),
            )?;
            scenario.stages_started = true;
            Ok(())
        });

        methods.add_method_mut("on_stop", |_, scenario, callback: String| {
            if scenario.stop_handler_registered {
                return Err(mlua::Error::RuntimeError(format!(
                    "Scenario '{}' stop handler is already defined",
                    scenario.id.as_str(),
                )));
            }
            let callback = validate_callback(callback)?;
            send_application_command(
                &scenario.command_sender,
                ScenarioCommand::OnStop {
                    id: scenario.id.clone(),
                    callback,
                }
                .into(),
            )?;
            scenario.stop_handler_registered = true;
            Ok(())
        });

        methods.add_method_mut("on_error", |_, scenario, callback: String| {
            if scenario.error_handler_registered {
                return Err(mlua::Error::RuntimeError(format!(
                    "Scenario '{}' error handler is already defined",
                    scenario.id.as_str(),
                )));
            }
            let callback = validate_callback(callback)?;
            send_application_command(
                &scenario.command_sender,
                ScenarioCommand::OnError {
                    id: scenario.id.clone(),
                    callback,
                }
                .into(),
            )?;
            scenario.error_handler_registered = true;
            Ok(())
        });

        methods.add_method("complete", |_, scenario, reason: String| {
            send_application_command(
                &scenario.command_sender,
                ScenarioCommand::Complete {
                    id: scenario.id.clone(),
                    reason: validate_reason(reason)?,
                }
                .into(),
            )
        });

        methods.add_method("stop", |_, scenario, reason: String| {
            send_application_command(
                &scenario.command_sender,
                ScenarioCommand::Stop {
                    id: scenario.id.clone(),
                    reason: validate_reason(reason)?,
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
    let alternatives = parse_table_array(options, "Scenario race")?;
    if alternatives.is_empty() {
        return Err(mlua::Error::RuntimeError(
            "Scenario race must contain at least one alternative".to_owned(),
        ));
    }

    alternatives
        .into_iter()
        .map(|alternative| parse_race_alternative(&alternative))
        .collect()
}

fn parse_race_alternative(options: &Table) -> mlua::Result<ScenarioRaceAlternative> {
    validate_keys(
        options,
        "scenario race alternative",
        &["after", "at", "when", "callback"],
    )?;

    Ok(ScenarioRaceAlternative {
        trigger: parse_trigger(options, "Scenario race alternative")?,
        callback: validate_callback(required_string(
            options,
            "callback",
            "scenario race alternative",
        )?)?,
    })
}

fn parse_stage_definition(
    options: &Table,
    name: &ScenarioStageName,
) -> mlua::Result<ScenarioStageDefinition> {
    let context = format!("scenario stage '{}'", name.as_str());
    validate_keys(options, &context, &["enter", "transitions"])?;
    let enter = validate_callback(required_string(options, "enter", &context)?)?;
    let transitions = match options.get::<Option<Table>>("transitions")? {
        Some(transitions) => parse_stage_transitions(&transitions, name)?,
        None => Vec::new(),
    };

    Ok(ScenarioStageDefinition { enter, transitions })
}

fn parse_stage_transitions(
    options: &Table,
    stage: &ScenarioStageName,
) -> mlua::Result<Vec<ScenarioStageTransition>> {
    let context = format!("Transitions of scenario stage '{}'", stage.as_str());
    parse_table_array(options, &context)?
        .into_iter()
        .enumerate()
        .map(|(index, transition)| {
            let transition_context = format!(
                "transition #{} of scenario stage '{}'",
                index + 1,
                stage.as_str(),
            );
            validate_keys(
                &transition,
                &transition_context,
                &["after", "at", "when", "next", "reason"],
            )?;
            let next = required_string(&transition, "next", &transition_context)?;
            let next = ScenarioStageName::new(next)
                .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;
            let reason = transition.get::<Option<String>>("reason")?;
            if reason
                .as_ref()
                .is_some_and(|reason| reason.trim().is_empty())
            {
                return Err(mlua::Error::RuntimeError(format!(
                    "{transition_context} reason cannot be empty",
                )));
            }

            Ok(ScenarioStageTransition {
                trigger: parse_trigger(&transition, &transition_context)?,
                next,
                reason,
            })
        })
        .collect()
}

fn parse_trigger(options: &Table, context: &str) -> mlua::Result<ScenarioRaceTrigger> {
    let after = options.get::<Option<f64>>("after")?;
    let at = options.get::<Option<f64>>("at")?;
    let when = options.get::<Option<Table>>("when")?;
    let trigger_count =
        usize::from(after.is_some()) + usize::from(at.is_some()) + usize::from(when.is_some());

    if trigger_count != 1 {
        return Err(mlua::Error::RuntimeError(format!(
            "{context} must define exactly one of 'after', 'at' or 'when'",
        )));
    }

    if let Some(seconds) = after {
        Ok(ScenarioRaceTrigger::After {
            delay: parse_duration(seconds, &format!("{context} delay"))?,
        })
    } else if let Some(timestamp) = at {
        Ok(ScenarioRaceTrigger::At {
            deadline: parse_unix_timestamp(timestamp)?,
        })
    } else {
        Ok(ScenarioRaceTrigger::When {
            condition: parse_condition(&when.expect("trigger count was validated"))?,
        })
    }
}

fn parse_table_array(options: &Table, context: &str) -> mlua::Result<Vec<Table>> {
    let mut indexed_entries = Vec::new();

    for pair in options.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Value::Integer(index) = key else {
            return Err(mlua::Error::RuntimeError(format!(
                "{context} must be an array without named keys",
            )));
        };
        if index <= 0 {
            return Err(mlua::Error::RuntimeError(format!(
                "{context} array indices must start at 1",
            )));
        }
        let Value::Table(entry) = value else {
            return Err(mlua::Error::RuntimeError(format!(
                "{context} entry #{index} must be a table",
            )));
        };

        indexed_entries.push((index, entry));
    }

    indexed_entries.sort_by_key(|(index, _)| *index);

    for (offset, (index, _)) in indexed_entries.iter().enumerate() {
        let expected = i64::try_from(offset + 1).expect("Lua table index must fit i64");
        if *index != expected {
            return Err(mlua::Error::RuntimeError(format!(
                "{context} entries must use consecutive array indices starting at 1",
            )));
        }
    }

    Ok(indexed_entries
        .into_iter()
        .map(|(_, entry)| entry)
        .collect())
}

fn parse_condition(options: &Table) -> mlua::Result<ScenarioCondition> {
    parse_condition_at_depth(options, 0)
}

fn parse_condition_at_depth(options: &Table, depth: usize) -> mlua::Result<ScenarioCondition> {
    validate_keys(
        options,
        "scenario condition",
        &[
            "series",
            "above",
            "below",
            "inside",
            "outside",
            "stable",
            "rate_above",
            "rate_below",
            "window_seconds",
            "stale_for_seconds",
            "all",
            "any",
            "for_seconds",
            "hysteresis",
            "edge",
        ],
    )?;

    let above = options.get::<Option<f64>>("above")?;
    let below = options.get::<Option<f64>>("below")?;
    let inside = options.get::<Option<Table>>("inside")?;
    let outside = options.get::<Option<Table>>("outside")?;
    let stable = options.get::<Option<Table>>("stable")?;
    let rate_above = options.get::<Option<f64>>("rate_above")?;
    let rate_below = options.get::<Option<f64>>("rate_below")?;
    let stale = options.get::<Option<f64>>("stale_for_seconds")?;
    let all = options.get::<Option<Table>>("all")?;
    let any = options.get::<Option<Table>>("any")?;
    let operator_count = [
        above.is_some(),
        below.is_some(),
        inside.is_some(),
        outside.is_some(),
        stable.is_some(),
        rate_above.is_some(),
        rate_below.is_some(),
        stale.is_some(),
        all.is_some(),
        any.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();

    if operator_count != 1 {
        return Err(mlua::Error::RuntimeError(
            "Scenario condition must define exactly one condition operator".to_owned(),
        ));
    }

    let hold = parse_duration(
        options.get::<Option<f64>>("for_seconds")?.unwrap_or(0.0),
        "Scenario condition for_seconds",
    )?;
    let hysteresis_option = options.get::<Option<f64>>("hysteresis")?;
    let hysteresis = hysteresis_option.unwrap_or(0.0);
    validate_non_negative_finite(hysteresis, "Scenario condition hysteresis")?;

    let edge = options
        .get::<Option<String>>("edge")?
        .unwrap_or_else(|| "rising".to_owned());

    if edge != "rising" {
        return Err(mlua::Error::RuntimeError(format!(
            "Unsupported scenario condition edge '{edge}'; expected 'rising'",
        )));
    }

    let composite = all.or(any);
    if let Some(children) = composite {
        if depth >= 3 {
            return Err(mlua::Error::RuntimeError(
                "Scenario condition composition cannot exceed 4 levels".to_owned(),
            ));
        }
        if options.contains_key("series")?
            || options.contains_key("window_seconds")?
            || hysteresis_option.is_some()
        {
            return Err(mlua::Error::RuntimeError(
                "Composite scenario conditions cannot define 'series', 'window_seconds' or 'hysteresis'"
                    .to_owned(),
            ));
        }
        let children = parse_table_array(&children, "Scenario condition children")?;
        if children.is_empty() || children.len() > 16 {
            return Err(mlua::Error::RuntimeError(
                "Scenario condition composition must contain between 1 and 16 children".to_owned(),
            ));
        }
        let children = children
            .into_iter()
            .map(|child| parse_condition_at_depth(&child, depth + 1))
            .collect::<mlua::Result<Vec<_>>>()?;
        let kind = if options.contains_key("all")? {
            ScenarioConditionKind::All(children)
        } else {
            ScenarioConditionKind::Any(children)
        };

        return Ok(ScenarioCondition { kind, hold });
    }

    let series = required_string(options, "series", "scenario condition")?;
    let window = options.get::<Option<f64>>("window_seconds")?;

    let kind = if let Some(threshold) = above {
        reject_window(window)?;
        ScenarioConditionKind::Threshold {
            series,
            direction: ThresholdDirection::Above,
            threshold: finite_number(threshold, "Scenario threshold")?,
            hysteresis,
        }
    } else if let Some(threshold) = below {
        reject_window(window)?;
        ScenarioConditionKind::Threshold {
            series,
            direction: ThresholdDirection::Below,
            threshold: finite_number(threshold, "Scenario threshold")?,
            hysteresis,
        }
    } else if let Some(range) = inside {
        reject_window(window)?;
        let (minimum, maximum) = parse_range(&range, "Scenario inside range")?;
        ScenarioConditionKind::Inside {
            series,
            minimum,
            maximum,
            hysteresis,
        }
    } else if let Some(range) = outside {
        reject_window(window)?;
        let (minimum, maximum) = parse_range(&range, "Scenario outside range")?;
        if hysteresis * 2.0 >= maximum - minimum && hysteresis > 0.0 {
            return Err(mlua::Error::RuntimeError(
                "Scenario outside hysteresis must be less than half the range width".to_owned(),
            ));
        }
        ScenarioConditionKind::Outside {
            series,
            minimum,
            maximum,
            hysteresis,
        }
    } else if let Some(stable) = stable {
        reject_window(window)?;
        if hold.is_zero() {
            return Err(mlua::Error::RuntimeError(
                "Scenario stable condition requires positive 'for_seconds'".to_owned(),
            ));
        }
        validate_keys(
            &stable,
            "scenario stable condition",
            &["target", "tolerance"],
        )?;
        let target = required_finite_number(&stable, "target", "scenario stable condition")?;
        let tolerance = required_finite_number(&stable, "tolerance", "scenario stable condition")?;
        if tolerance <= 0.0 {
            return Err(mlua::Error::RuntimeError(
                "Scenario stable tolerance must be positive".to_owned(),
            ));
        }
        ScenarioConditionKind::Stable {
            series,
            target,
            tolerance,
            hysteresis,
        }
    } else if let Some(threshold) = rate_above.or(rate_below) {
        let Some(window) = window else {
            return Err(mlua::Error::RuntimeError(
                "Scenario rate condition requires positive 'window_seconds'".to_owned(),
            ));
        };
        let window = parse_duration(window, "Scenario rate window_seconds")?;
        if window.is_zero() {
            return Err(mlua::Error::RuntimeError(
                "Scenario rate condition requires positive 'window_seconds'".to_owned(),
            ));
        }
        ScenarioConditionKind::Rate {
            series,
            direction: if rate_above.is_some() {
                ThresholdDirection::Above
            } else {
                ThresholdDirection::Below
            },
            threshold: finite_number(threshold, "Scenario rate threshold")?,
            window,
            hysteresis,
        }
    } else {
        if !hold.is_zero() || hysteresis_option.is_some() || window.is_some() {
            return Err(mlua::Error::RuntimeError(
                "Scenario stale condition cannot define 'for_seconds', 'hysteresis' or 'window_seconds'"
                    .to_owned(),
            ));
        }
        let duration = parse_duration(
            stale.expect("condition operator count was validated"),
            "Scenario stale_for_seconds",
        )?;
        if duration.is_zero() {
            return Err(mlua::Error::RuntimeError(
                "Scenario stale_for_seconds must be positive".to_owned(),
            ));
        }
        ScenarioConditionKind::Stale { series, duration }
    };

    Ok(ScenarioCondition { kind, hold })
}

fn parse_range(options: &Table, context: &str) -> mlua::Result<(f64, f64)> {
    validate_keys(options, context, &["min", "max"])?;
    let minimum = required_finite_number(options, "min", context)?;
    let maximum = required_finite_number(options, "max", context)?;
    if minimum >= maximum {
        return Err(mlua::Error::RuntimeError(format!(
            "{context} min must be less than max",
        )));
    }
    Ok((minimum, maximum))
}

fn required_finite_number(table: &Table, key: &str, context: &str) -> mlua::Result<f64> {
    let value = table.get::<Option<f64>>(key)?.ok_or_else(|| {
        mlua::Error::RuntimeError(format!("{context} must contain number '{key}'"))
    })?;
    finite_number(value, &format!("{context} {key}"))
}

fn finite_number(value: f64, context: &str) -> mlua::Result<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(mlua::Error::RuntimeError(format!(
            "{context} must be finite",
        )))
    }
}

fn validate_non_negative_finite(value: f64, context: &str) -> mlua::Result<()> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(mlua::Error::RuntimeError(format!(
            "{context} must be finite and non-negative",
        )))
    }
}

fn reject_window(window: Option<f64>) -> mlua::Result<()> {
    if window.is_some() {
        Err(mlua::Error::RuntimeError(
            "Scenario window_seconds is only valid with rate_above or rate_below".to_owned(),
        ))
    } else {
        Ok(())
    }
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

fn validate_reason(reason: String) -> mlua::Result<String> {
    if reason.trim().is_empty() {
        return Err(mlua::Error::RuntimeError(
            "Scenario termination reason cannot be empty".to_owned(),
        ));
    }

    Ok(reason)
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
        scenario::{
            ScenarioCommand, ScenarioConditionKind, ScenarioRaceTrigger, ThresholdDirection,
        },
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
                scenario:stage("heating", {
                    enter = "start_heating",
                    transitions = {
                        {
                            when = {
                                series = "temperature",
                                above = 150.0,
                            },
                            next = "holding",
                        },
                        {
                            after = 1800.0,
                            next = "failed",
                            reason = "Heating timeout",
                        },
                    },
                })
                scenario:stage("holding", { enter = "start_holding" })
                scenario:stage("failed", { enter = "handle_failure" })
                scenario:start("heating")
                scenario:on_stop("cleanup")
                scenario:on_error("recover")
                scenario:complete("normal completion")
                scenario:stop("operator requested stop")
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
        assert_eq!(condition.hold, Duration::from_secs(5));
        assert!(matches!(
            condition.kind,
            ScenarioConditionKind::Threshold {
                series,
                direction: ThresholdDirection::Above,
                threshold: 150.0,
                hysteresis: 2.0,
            } if series == "temperature"
        ));
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
                if matches!(
                    &condition.kind,
                    ScenarioConditionKind::Threshold {
                        series,
                        direction: ThresholdDirection::Above,
                        threshold: 180.0,
                        ..
                    } if series == "temperature"
                )
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

        let UserCommand::Scenario(ScenarioCommand::DefineStage {
            id,
            name,
            definition,
        }) = receiver.try_recv().unwrap()
        else {
            panic!("expected heating stage definition");
        };
        assert_eq!(id.as_str(), "heat_cycle");
        assert_eq!(name.as_str(), "heating");
        assert_eq!(definition.enter, "start_heating");
        assert_eq!(definition.transitions.len(), 2);
        assert_eq!(definition.transitions[0].next.as_str(), "holding");
        assert_eq!(definition.transitions[1].next.as_str(), "failed");
        assert_eq!(
            definition.transitions[1].reason.as_deref(),
            Some("Heating timeout")
        );

        for (expected_name, expected_callback) in
            [("holding", "start_holding"), ("failed", "handle_failure")]
        {
            assert!(matches!(
                receiver.try_recv().unwrap(),
                UserCommand::Scenario(ScenarioCommand::DefineStage {
                    id,
                    name,
                    definition,
                }) if id.as_str() == "heat_cycle"
                    && name.as_str() == expected_name
                    && definition.enter == expected_callback
                    && definition.transitions.is_empty()
            ));
        }

        assert!(matches!(
            receiver.try_recv().unwrap(),
            UserCommand::Scenario(ScenarioCommand::Start { id, stage })
                if id.as_str() == "heat_cycle" && stage.as_str() == "heating"
        ));

        assert!(matches!(
            receiver.try_recv().unwrap(),
            UserCommand::Scenario(ScenarioCommand::OnStop { id, callback })
                if id.as_str() == "heat_cycle" && callback == "cleanup"
        ));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            UserCommand::Scenario(ScenarioCommand::OnError { id, callback })
                if id.as_str() == "heat_cycle" && callback == "recover"
        ));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            UserCommand::Scenario(ScenarioCommand::Complete { id, reason })
                if id.as_str() == "heat_cycle" && reason == "normal completion"
        ));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            UserCommand::Scenario(ScenarioCommand::Stop { id, reason })
                if id.as_str() == "heat_cycle" && reason == "operator requested stop"
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

        assert!(error.to_string().contains("exactly one condition operator"));
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
    fn parses_extended_scenario_conditions() {
        let lua = Lua::new();
        let app = lua.create_table().unwrap();
        let (sender, receiver) = unbounded();
        register_scenario(&lua, &app, sender).unwrap();
        lua.globals().set("app", app).unwrap();

        lua.load(
            r#"
                local scenario = app.scenario({ id = "conditions" })
                scenario:when({
                    series = "temperature",
                    inside = { min = 90.0, max = 110.0 },
                    hysteresis = 1.0,
                }, "inside")
                scenario:when({
                    series = "temperature",
                    outside = { min = 0.0, max = 200.0 },
                }, "outside")
                scenario:when({
                    series = "temperature",
                    stable = { target = 100.0, tolerance = 0.5 },
                    for_seconds = 10.0,
                }, "stable")
                scenario:when({
                    series = "temperature",
                    rate_above = 2.0,
                    window_seconds = 15.0,
                }, "rate")
                scenario:when({
                    series = "pressure",
                    rate_below = -0.5,
                    window_seconds = 3.0,
                }, "rate_below")
                scenario:when({
                    series = "temperature",
                    stale_for_seconds = 5.0,
                }, "stale")
                scenario:when({
                    all = {
                        { series = "temperature", above = 100.0 },
                        { series = "pressure", below = 10.0 },
                    },
                }, "all")
                scenario:when({
                    any = {
                        { series = "temperature", above = 200.0 },
                        { series = "pressure", below = 1.0 },
                    },
                }, "any")
            "#,
        )
        .exec()
        .unwrap();

        let _register = receiver.try_recv().unwrap();
        let conditions = (0..8)
            .map(|_| {
                let UserCommand::Scenario(ScenarioCommand::When { condition, .. }) =
                    receiver.try_recv().unwrap()
                else {
                    panic!("expected scenario condition command");
                };
                condition
            })
            .collect::<Vec<_>>();

        assert!(matches!(
            conditions[0].kind,
            ScenarioConditionKind::Inside {
                minimum: 90.0,
                maximum: 110.0,
                hysteresis: 1.0,
                ..
            }
        ));
        assert!(matches!(
            conditions[1].kind,
            ScenarioConditionKind::Outside {
                minimum: 0.0,
                maximum: 200.0,
                ..
            }
        ));
        assert!(matches!(
            conditions[2].kind,
            ScenarioConditionKind::Stable {
                target: 100.0,
                tolerance: 0.5,
                ..
            }
        ));
        assert_eq!(conditions[2].hold, Duration::from_secs(10));
        assert!(matches!(
            conditions[3].kind,
            ScenarioConditionKind::Rate {
                direction: ThresholdDirection::Above,
                threshold: 2.0,
                window,
                ..
            } if window == Duration::from_secs(15)
        ));
        assert!(matches!(
            conditions[4].kind,
            ScenarioConditionKind::Rate {
                direction: ThresholdDirection::Below,
                threshold: -0.5,
                window,
                ..
            } if window == Duration::from_secs(3)
        ));
        assert!(matches!(
            conditions[5].kind,
            ScenarioConditionKind::Stale { duration, .. }
                if duration == Duration::from_secs(5)
        ));
        assert!(matches!(
            &conditions[6].kind,
            ScenarioConditionKind::All(children) if children.len() == 2
        ));
        assert!(matches!(
            &conditions[7].kind,
            ScenarioConditionKind::Any(children) if children.len() == 2
        ));
    }

    #[test]
    fn rejects_invalid_extended_scenario_conditions() {
        for (source, expected) in [
            (
                "scenario:when({ series='x', stable={target=1,tolerance=0.1} }, 'cb')",
                "requires positive 'for_seconds'",
            ),
            (
                "scenario:when({ series='x', rate_above=1 }, 'cb')",
                "requires positive 'window_seconds'",
            ),
            (
                "scenario:when({ series='x', stale_for_seconds=1, hysteresis=1 }, 'cb')",
                "stale condition cannot define",
            ),
            (
                "scenario:when({ all={} }, 'cb')",
                "between 1 and 16 children",
            ),
            (
                "scenario:when({ series='x', inside={min=2,max=1} }, 'cb')",
                "min must be less than max",
            ),
            (
                "scenario:when({ series='x', above=0/0 }, 'cb')",
                "must be finite",
            ),
        ] {
            let error = execute_invalid_stage(source);
            assert!(error.contains(expected), "{error}");
        }
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

    #[test]
    fn rejects_duplicate_scenario_stage() {
        let error = execute_invalid_stage(
            r#"
                scenario:stage("heating", { enter = "first" })
                scenario:stage("heating", { enter = "second" })
            "#,
        );

        assert!(error.contains("duplicate stage 'heating'"));
    }

    #[test]
    fn rejects_unknown_stage_transition_target() {
        let error = execute_invalid_stage(
            r#"
                scenario:stage("heating", {
                    enter = "start_heating",
                    transitions = {{ after = 1.0, next = "missing" }},
                })
                scenario:start("heating")
            "#,
        );

        assert!(error.contains("transition to unknown stage 'missing'"));
    }

    #[test]
    fn rejects_unknown_start_stage() {
        let error = execute_invalid_stage("scenario:start('missing')");

        assert!(error.contains("start stage 'missing' is not defined"));
    }

    #[test]
    fn rejects_repeated_scenario_start() {
        let error = execute_invalid_stage(
            r#"
                scenario:stage("heating", { enter = "start_heating" })
                scenario:start("heating")
                scenario:start("heating")
            "#,
        );

        assert!(error.contains("stages have already been started"));
    }

    #[test]
    fn rejects_invalid_scenario_finalization() {
        let error = execute_invalid_stage("scenario:complete('   ')");
        assert!(error.contains("termination reason cannot be empty"));

        let error =
            execute_invalid_stage("scenario:on_stop('cleanup'); scenario:on_stop('cleanup_again')");
        assert!(error.contains("stop handler is already defined"));
    }

    fn execute_invalid_race(source: &str) -> String {
        execute_invalid_stage(source)
    }

    fn execute_invalid_stage(source: &str) -> String {
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
