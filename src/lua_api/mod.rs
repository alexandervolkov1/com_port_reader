//! Lua bindings installed as the global `app` table.
//!
//! Lua code only publishes commands and UI events; [`crate::application_runtime`] receives and
//! performs them. This keeps scripts from directly owning workers or hardware connections.

mod controllers;
mod conversion;
mod filters;
mod metakon;
mod scenarios;
mod series;
mod virtual_instrument;

use std::cell::RefCell;

use crossbeam_channel::Sender;
use mlua::{Lua, Table, Value};

#[cfg(test)]
use self::{
    controllers::{
        LuaControllerHandle, add_furnace_loop, add_on_off_loop, validate_on_off_options,
    },
    metakon::LuaMetakon5x3,
    virtual_instrument::LuaVirtualInstrument,
};
use self::{
    filters::{register_add_filter, register_set_filter},
    metakon::register_metakon_controller,
    scenarios::register_scenario,
    series::{
        connection_id_from_options, register_add_serial, register_delete_series,
        register_rename_series, register_retry_series, register_set_series_color,
        register_set_series_pane,
    },
    virtual_instrument::register_virtual_instrument_controller,
};
use crate::{
    application_definition::ApplicationDefinition,
    connection::ConnectionId,
    lua_application_script::LuaApplicationEvent,
    scenario::{ScenarioId, ScenarioRunId},
    user_command::{
        AcquisitionCommand, EmulatorCommand, SerialCommand, SeriesCommand, UserCommand,
    },
};

thread_local! {
    static SCENARIO_COMMAND_CONTEXT: RefCell<Option<(ScenarioId, ScenarioRunId)>> = const { RefCell::new(None) };
}

struct ScenarioCommandContextGuard(Option<(ScenarioId, ScenarioRunId)>);

impl Drop for ScenarioCommandContextGuard {
    fn drop(&mut self) {
        SCENARIO_COMMAND_CONTEXT.with(|context| {
            context.replace(self.0.take());
        });
    }
}

pub(crate) fn with_scenario_command_context<T>(
    scenario_id: ScenarioId,
    run_id: ScenarioRunId,
    callback: impl FnOnce() -> T,
) -> T {
    let previous =
        SCENARIO_COMMAND_CONTEXT.with(|context| context.replace(Some((scenario_id, run_id))));
    let _guard = ScenarioCommandContextGuard(previous);
    callback()
}

pub fn install(
    lua: &Lua,
    command_sender: Sender<UserCommand>,
    application_event_sender: Sender<LuaApplicationEvent>,
    application_definition: &ApplicationDefinition,
) -> mlua::Result<()> {
    let app = lua.create_table()?;

    register_command(lua, &app, "start", command_sender.clone(), start_command)?;

    register_command(lua, &app, "stop", command_sender.clone(), stop_command)?;

    register_command(lua, &app, "clear", command_sender.clone(), clear_command)?;

    register_command(
        lua,
        &app,
        "start_emu",
        command_sender.clone(),
        start_emulator_command,
    )?;

    register_command(
        lua,
        &app,
        "stop_emu",
        command_sender.clone(),
        stop_emulator_command,
    )?;

    register_log(lua, &app, command_sender.clone())?;

    register_scenario(lua, &app, command_sender.clone())?;

    register_add_serial(
        lua,
        &app,
        command_sender.clone(),
        application_definition.clone(),
    )?;

    register_add_filter(lua, &app, command_sender.clone())?;

    register_set_filter(lua, &app, command_sender.clone())?;

    register_metakon_controller(
        lua,
        &app,
        command_sender.clone(),
        application_definition.clone(),
    )?;

    register_virtual_instrument_controller(
        lua,
        &app,
        command_sender.clone(),
        application_definition.clone(),
    )?;

    register_delete_series(lua, &app, command_sender.clone())?;

    register_rename_series(lua, &app, command_sender.clone())?;

    register_set_series_color(lua, &app, command_sender.clone())?;

    register_set_series_pane(lua, &app, command_sender.clone())?;

    register_retry_series(lua, &app, command_sender.clone())?;

    register_command(
        lua,
        &app,
        "retry_all",
        command_sender.clone(),
        retry_all_command,
    )?;

    register_send_serial(lua, &app, command_sender, application_definition.clone())?;

    crate::lua_application_script::install(lua, &app, application_event_sender)?;

    lua.globals().set("app", app)
}

fn register_command(
    lua: &Lua,
    app: &Table,
    name: &str,
    command_sender: Sender<UserCommand>,
    command_factory: fn() -> UserCommand,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, ()| {
        send_application_command(&command_sender, command_factory())
    })?;

    app.set(name, function)
}

fn register_log(lua: &Lua, app: &Table, command_sender: Sender<UserCommand>) -> mlua::Result<()> {
    let function = lua.create_function(move |_, message: String| {
        send_application_command(&command_sender, UserCommand::Log { message })
    })?;

    app.set("log", function)
}

fn validate_send_serial_options(options: &Table) -> mlua::Result<()> {
    for pair in options.pairs::<String, Value>() {
        let (key, _) = pair?;

        if key != "connection" {
            return Err(mlua::Error::RuntimeError(format!(
                "unknown app.send_serial \
                         option '{key}'",
            )));
        }
    }

    Ok(())
}

fn register_send_serial(
    lua: &Lua,
    app: &Table,
    command_sender: Sender<UserCommand>,
    application_definition: ApplicationDefinition,
) -> mlua::Result<()> {
    let function = lua.create_function(move |_, (command, options): (String, Option<Table>)| {
        let connection_id = match options {
            Some(options) => {
                validate_send_serial_options(&options)?;

                connection_id_from_options(&options, &application_definition)?
            }

            None => ConnectionId::PRIMARY,
        };

        send_application_command(
            &command_sender,
            SerialCommand::SendText {
                connection_id,
                command,
            }
            .into(),
        )
    })?;

    app.set("send_serial", function)
}

pub(super) fn send_application_command(
    command_sender: &Sender<UserCommand>,
    command: UserCommand,
) -> mlua::Result<()> {
    let scenario = SCENARIO_COMMAND_CONTEXT.with(|context| context.borrow().clone());
    let command = match scenario {
        Some((scenario_id, run_id)) => UserCommand::ScenarioStep {
            scenario_id,
            run_id,
            command: Box::new(command),
        },
        None => command,
    };

    command_sender.send(command).map_err(|_| {
        mlua::Error::RuntimeError("application command channel is disconnected".to_owned())
    })
}

fn start_command() -> UserCommand {
    AcquisitionCommand::Start.into()
}

fn stop_command() -> UserCommand {
    AcquisitionCommand::Stop.into()
}

fn clear_command() -> UserCommand {
    SeriesCommand::Clear.into()
}

fn start_emulator_command() -> UserCommand {
    EmulatorCommand::Start.into()
}

fn stop_emulator_command() -> UserCommand {
    EmulatorCommand::Stop.into()
}

fn retry_all_command() -> UserCommand {
    SeriesCommand::RetryAll.into()
}

#[cfg(test)]
mod furnace_tests;

#[cfg(test)]
mod controller_handle_tests {
    use std::thread;

    use crossbeam_channel::unbounded;
    use mlua::Lua;

    use super::{LuaControllerHandle, add_on_off_loop, validate_on_off_options};
    use crate::{
        connection::ConnectionId,
        data::SeriesColor,
        instrument::{
            InstrumentValue, ParameterAccess, ParameterValueType,
            virtual_instrument::{
                VirtualInstrumentId, VirtualParameterDescriptor, VirtualParameterId,
            },
        },
        process_control::{
            ControlLoopState, ControlOutputTarget, ControllerDiagnostic, ReferenceKind,
            ReferenceSource,
        },
        user_command::{ControllerCommand, UserCommand},
    };

    #[test]
    fn adds_controller_diagnostic_series() {
        let lua = Lua::new();

        let (command_sender, command_receiver) = unbounded();

        let controller = LuaControllerHandle {
            name: "heater".to_owned(),
            connection_id: ConnectionId::new(2),
            command_sender,
        };

        let userdata = lua.create_userdata(controller).unwrap();

        lua.globals().set("controller", userdata).unwrap();

        let responder = thread::spawn(move || {
            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::Diagnostics {
                name,
                response_sender,
            }) = command
            else {
                panic!("expected ControllerDiagnostics command");
            };

            assert_eq!(name, "heater");

            response_sender
                .send(Ok(ControllerDiagnostic::ALL.to_vec()))
                .unwrap();

            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::AddDiagnostic(series)) = command else {
                panic!("expected AddControllerDiagnostic command");
            };

            assert!(command_receiver.try_recv().is_err());

            series
        });

        lua.load(
            r##"
                    controller:add(
                        "integral",
                        {
                            name = "heater_i",
                            color = "#112233",
                            visible = false,
                            pane = "control",
                        }
                    )
                "##,
        )
        .exec()
        .unwrap();

        let series = responder.join().unwrap();

        let (controller, diagnostic, name, connection_id, presentation) = series.into_parts();

        assert_eq!(controller, "heater");

        assert_eq!(diagnostic, ControllerDiagnostic::Integral,);

        assert_eq!(name, "heater_i");

        assert_eq!(connection_id, ConnectionId::new(2),);

        assert_eq!(
            presentation.color,
            Some(SeriesColor::new(0x11, 0x22, 0x33,),),
        );
        assert!(!presentation.visible);
        assert_eq!(
            presentation.pane.as_ref().map(|pane| pane.as_str()),
            Some("control"),
        );
    }

    #[test]
    fn generates_controller_diagnostic_series_name() {
        let lua = Lua::new();

        let (command_sender, command_receiver) = unbounded();

        let userdata = lua
            .create_userdata(LuaControllerHandle {
                name: "heater".to_owned(),
                connection_id: ConnectionId::PRIMARY,
                command_sender,
            })
            .unwrap();

        lua.globals().set("controller", userdata).unwrap();

        let responder = thread::spawn(move || {
            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::Diagnostics {
                name,
                response_sender,
            }) = command
            else {
                panic!("expected ControllerDiagnostics command");
            };

            assert_eq!(name, "heater");

            response_sender
                .send(Ok(ControllerDiagnostic::ALL.to_vec()))
                .unwrap();

            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::AddDiagnostic(series)) = command else {
                panic!("expected AddControllerDiagnostic command");
            };

            assert!(command_receiver.try_recv().is_err());

            series
        });

        lua.load(
            r#"
                    controller:add(
                        "proportional"
                    )
                "#,
        )
        .exec()
        .unwrap();

        let series = responder.join().unwrap();

        let (controller, diagnostic, name, connection_id, presentation) = series.into_parts();

        assert_eq!(controller, "heater");

        assert_eq!(diagnostic, ControllerDiagnostic::Proportional,);

        assert_eq!(name, "heater_proportional",);

        assert_eq!(connection_id, ConnectionId::PRIMARY,);

        assert_eq!(presentation.color, None);
    }

    #[test]
    fn rejects_controller_diagnostic_interval() {
        let lua = Lua::new();

        let (command_sender, command_receiver) = unbounded();

        let userdata = lua
            .create_userdata(LuaControllerHandle {
                name: "heater".to_owned(),

                connection_id: ConnectionId::PRIMARY,

                command_sender,
            })
            .unwrap();

        lua.globals().set("controller", userdata).unwrap();

        let error = lua
            .load(
                r#"
                        controller:add(
                            "integral",
                            {
                                interval = 1.0,
                            }
                        )
                    "#,
            )
            .exec()
            .unwrap_err()
            .to_string();

        assert!(
            error.contains("cannot have 'interval'",),
            "unexpected Lua error: \
                 {error}",
        );

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn changes_controller_input() {
        let lua = Lua::new();

        let (command_sender, command_receiver) = unbounded();

        let userdata = lua
            .create_userdata(LuaControllerHandle {
                name: "heater".to_owned(),
                connection_id: ConnectionId::PRIMARY,
                command_sender,
            })
            .unwrap();

        lua.globals().set("controller", userdata).unwrap();

        let responder = std::thread::spawn(move || {
            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::SetInput {
                name,
                input_name,
                response_sender,
            }) = command
            else {
                panic!(
                    "expected SetControllerInput \
                             command"
                );
            };

            assert_eq!(name, "heater");

            assert_eq!(input_name, "temperature_filtered",);

            response_sender.send(Ok(())).unwrap();
        });

        lua.load(
            r#"
                    controller:set_input(
                        "temperature_filtered"
                    )
                "#,
        )
        .exec()
        .unwrap();

        responder.join().unwrap();
    }

    #[test]
    fn controls_controller_lifecycle() {
        let lua = Lua::new();

        let (command_sender, command_receiver) = unbounded();

        let userdata = lua
            .create_userdata(LuaControllerHandle {
                name: "heater".to_owned(),
                connection_id: ConnectionId::PRIMARY,
                command_sender,
            })
            .unwrap();

        lua.globals().set("controller", userdata).unwrap();

        let responder = std::thread::spawn(move || {
            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::State {
                name,
                response_sender,
            }) = command
            else {
                panic!("expected ControllerState command");
            };

            assert_eq!(name, "heater");

            response_sender.send(Ok(ControlLoopState::Running)).unwrap();

            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::Pause {
                name,
                response_sender,
            }) = command
            else {
                panic!("expected PauseController command");
            };

            assert_eq!(name, "heater");

            response_sender.send(Ok(())).unwrap();

            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::Resume {
                name,
                response_sender,
            }) = command
            else {
                panic!("expected ResumeController command");
            };

            assert_eq!(name, "heater");

            response_sender.send(Ok(())).unwrap();

            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::ResetIntegral {
                name,
                response_sender,
            }) = command
            else {
                panic!(
                    "expected \
                             ResetControllerIntegral command"
                );
            };

            assert_eq!(name, "heater");

            response_sender.send(Ok(())).unwrap();
        });

        lua.load(
            r#"
                    assert(
                        controller:state()
                            == "running"
                    )

                    controller:pause()
                    controller:resume()
                    controller:reset_integral()
                "#,
        )
        .exec()
        .unwrap();

        responder.join().unwrap();
    }

    #[test]
    fn rejects_unknown_on_off_option() {
        let lua = Lua::new();

        let options = lua.create_table().unwrap();

        options.set("banana", 42).unwrap();

        let error = validate_on_off_options(&options).unwrap_err().to_string();

        assert!(
            error.contains("Unknown on/off option 'banana'",),
            "unexpected Lua error: {error}",
        );
    }

    #[test]
    fn creates_on_off_controller_request() {
        let lua = Lua::new();

        let (command_sender, command_receiver) = unbounded();

        let parameter = VirtualParameterDescriptor::new(
            VirtualParameterId::new(2),
            "heater_power",
            "Heater power",
            ParameterAccess::ReadWrite,
            ParameterValueType::Number,
        );

        let output_target = ControlOutputTarget::virtual_instrument(
            ConnectionId::new(3),
            VirtualInstrumentId::new(1),
            &parameter,
        )
        .unwrap();

        let options = lua.create_table().unwrap();

        options.set("name", "thermostat").unwrap();

        options.set("input", "temperature").unwrap();

        options.set("setpoint", 150.0).unwrap();

        options.set("hysteresis", 2.0).unwrap();

        options.set("output_off", 0.0).unwrap();

        options.set("output_on", 100.0).unwrap();

        let handle = add_on_off_loop(&command_sender, output_target, &options).unwrap();

        assert_eq!(handle.name, "thermostat",);

        assert_eq!(handle.connection_id, ConnectionId::new(3),);

        let command = command_receiver.try_recv().unwrap();

        let UserCommand::Controller(ControllerCommand::Add(new_controller)) = command else {
            panic!("expected AddController command",);
        };

        assert_eq!(new_controller.name(), "thermostat",);

        assert_eq!(new_controller.input_name(), "temperature",);

        assert_eq!(
            new_controller.output_target().connection_id(),
            ConnectionId::new(3),
        );

        assert_eq!(new_controller.controller().kind().as_str(), "on_off",);

        assert_eq!(
            new_controller.controller().read("setpoint"),
            Ok(crate::instrument::InstrumentValue::Number(150.0,),),
        );

        assert_eq!(
            new_controller.controller().read("hysteresis"),
            Ok(crate::instrument::InstrumentValue::Number(2.0,),),
        );

        assert_eq!(
            new_controller.controller().read("output_off"),
            Ok(crate::instrument::InstrumentValue::Number(0.0,),),
        );

        assert_eq!(
            new_controller.controller().read("output_on"),
            Ok(crate::instrument::InstrumentValue::Number(100.0,),),
        );

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn rejects_unsupported_controller_diagnostic() {
        let lua = Lua::new();

        let (command_sender, command_receiver) = unbounded();

        let userdata = lua
            .create_userdata(LuaControllerHandle {
                name: "thermostat".to_owned(),
                connection_id: ConnectionId::PRIMARY,
                command_sender,
            })
            .unwrap();

        lua.globals().set("controller", userdata).unwrap();

        let responder = thread::spawn(move || {
            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::Diagnostics {
                name,
                response_sender,
            }) = command
            else {
                panic!(
                    "expected \
                             ControllerDiagnostics"
                );
            };

            assert_eq!(name, "thermostat",);

            response_sender
                .send(Ok(vec![
                    ControllerDiagnostic::Setpoint,
                    ControllerDiagnostic::Output,
                ]))
                .unwrap();

            command_receiver
        });

        let error = lua
            .load(
                r#"
                        controller:add(
                            "integral"
                        )
                    "#,
            )
            .exec()
            .unwrap_err()
            .to_string();

        assert!(
            error.contains(
                "does not support diagnostic \
                     'integral'",
            ),
            "unexpected Lua error: {error}",
        );

        let command_receiver = responder.join().unwrap();

        assert!(command_receiver.try_recv().is_err());
    }

    #[test]
    fn manages_controller_reference() {
        let lua = Lua::new();

        let (command_sender, command_receiver) = unbounded();

        let userdata = lua
            .create_userdata(LuaControllerHandle {
                name: "heater".to_owned(),
                connection_id: ConnectionId::PRIMARY,
                command_sender,
            })
            .unwrap();

        lua.globals().set("controller", userdata).unwrap();

        let responder = thread::spawn(move || {
            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::ReferenceKind {
                name,
                response_sender,
            }) = command
            else {
                panic!(
                    "expected \
                             ControllerReferenceKind"
                );
            };

            assert_eq!(name, "heater",);

            response_sender.send(Ok(Some(ReferenceKind::Ramp))).unwrap();

            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::ReferenceParameters {
                name,
                response_sender,
            }) = command
            else {
                panic!(
                    "expected \
                             ControllerReferenceParameters"
                );
            };

            assert_eq!(name, "heater",);

            response_sender
                .send(Ok(ReferenceSource::ramp(20.0, 150.0, 10.0)
                    .unwrap()
                    .parameters()))
                .unwrap();

            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::ReadReferenceParameter {
                name,
                key,
                response_sender,
            }) = command
            else {
                panic!(
                    "expected \
                             ReadControllerReferenceParameter"
                );
            };

            assert_eq!(name, "heater",);

            assert_eq!(key, "target",);

            response_sender
                .send(Ok(InstrumentValue::Number(150.0)))
                .unwrap();

            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::ReferenceParameters {
                response_sender,
                ..
            }) = command
            else {
                panic!(
                    "expected reference \
                             parameter discovery \
                             before write"
                );
            };

            response_sender
                .send(Ok(ReferenceSource::ramp(20.0, 150.0, 10.0)
                    .unwrap()
                    .parameters()))
                .unwrap();

            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::WriteReferenceParameter {
                name,
                key,
                value,
                response_sender,
            }) = command
            else {
                panic!(
                    "expected \
                             WriteControllerReferenceParameter"
                );
            };

            assert_eq!(name, "heater",);

            assert_eq!(key, "target",);

            assert_eq!(value, InstrumentValue::Number(200.0,),);

            response_sender
                .send(Ok(InstrumentValue::Number(200.0)))
                .unwrap();

            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::ReferenceParameters {
                response_sender,
                ..
            }) = command
            else {
                panic!(
                    "expected reference \
                             parameter discovery \
                             before configuration"
                );
            };

            response_sender
                .send(Ok(ReferenceSource::ramp(20.0, 200.0, 10.0)
                    .unwrap()
                    .parameters()))
                .unwrap();

            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::ConfigureReference {
                name,
                updates,
                response_sender,
            }) = command
            else {
                panic!(
                    "expected \
                             ConfigureControllerReference"
                );
            };

            assert_eq!(name, "heater",);

            assert_eq!(updates.len(), 2,);

            assert!(updates.contains(&("target".to_owned(), InstrumentValue::Number(250.0,),)),);

            assert!(updates.contains(&("rate".to_owned(), InstrumentValue::Number(5.0,),)),);

            response_sender.send(Ok(())).unwrap();

            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::SetReference {
                name,
                source,
                response_sender,
            }) = command
            else {
                panic!(
                    "expected \
                             SetControllerReference"
                );
            };

            assert_eq!(name, "heater",);

            assert_eq!(source, ReferenceSource::fixed(175.0,).unwrap(),);

            response_sender.send(Ok(())).unwrap();

            let command = command_receiver.recv().unwrap();

            let UserCommand::Controller(ControllerCommand::SetReference {
                name,
                source,
                response_sender,
            }) = command
            else {
                panic!(
                    "expected second \
                             SetControllerReference"
                );
            };

            assert_eq!(name, "heater",);

            assert_eq!(source, ReferenceSource::ramp(175.0, 220.0, 2.0,).unwrap(),);

            response_sender.send(Ok(())).unwrap();

            command_receiver
        });

        lua.load(
            r#"
                    assert(
                        controller:reference_kind()
                            == "ramp"
                    )

                    local parameters =
                        controller:
                            reference_parameters()

                    assert(
                        #parameters == 3
                    )

                    assert(
                        parameters[1].key
                            == "start"
                    )

                    assert(
                        parameters[2].key
                            == "target"
                    )

                    assert(
                        parameters[3].key
                            == "rate"
                    )

                    assert(
                        controller:
                            read_reference(
                                "target"
                            )
                            == 150
                    )

                    assert(
                        controller:
                            write_reference(
                                "target",
                                200
                            )
                            == 200
                    )

                    controller:
                        configure_reference({
                            target = 250,
                            rate = 5,
                        })

                    controller:
                        set_fixed_reference(
                            175
                        )

                    controller:
                        set_ramp_reference({
                            start = 175,
                            target = 220,
                            rate = 2,
                        })
                "#,
        )
        .exec()
        .unwrap();

        let command_receiver = responder.join().unwrap();

        assert!(command_receiver.try_recv().is_err(),);
    }

    #[test]
    fn rejects_unknown_ramp_reference_option() {
        let lua = Lua::new();

        let (command_sender, command_receiver) = unbounded();

        let userdata = lua
            .create_userdata(LuaControllerHandle {
                name: "heater".to_owned(),
                connection_id: ConnectionId::PRIMARY,
                command_sender,
            })
            .unwrap();

        lua.globals().set("controller", userdata).unwrap();

        let error = lua
            .load(
                r#"
                    controller:
                        set_ramp_reference({
                            start = 20,
                            target = 150,
                            rate = 2,
                            banana = 42,
                        })
                "#,
            )
            .exec()
            .unwrap_err()
            .to_string();

        assert!(
            error.contains(
                "Unknown ramp reference \
                 option 'banana'",
            ),
            "unexpected Lua error: {error}",
        );

        assert!(command_receiver.try_recv().is_err(),);
    }
}
