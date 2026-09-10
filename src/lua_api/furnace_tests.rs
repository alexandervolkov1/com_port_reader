use std::time::Duration;

use crossbeam_channel::unbounded;
use mlua::{Lua, Table};

use super::{LuaMetakon5x3, LuaVirtualInstrument, add_furnace_loop};
use crate::{
    connection::ConnectionId,
    instrument::{
        InstrumentValue, ParameterAccess, ParameterValueType,
        metakon_5x3::Metakon5x3,
        virtual_instrument::{
            VirtualInstrumentDescriptor, VirtualInstrumentId, VirtualParameterDescriptor,
            VirtualParameterId,
        },
    },
    process_control::{
        ControlEvent, ControlLoopDefinition, ControlOutputTarget, ControllerDiagnostic,
        ControllerKind, ControllerOutput,
    },
    signal_processing::{ProcessingEvent, ProcessingService},
    user_command::{ControllerCommand, UserCommand},
};

fn options(lua: &Lua) -> Table {
    lua.load(
        r#"return {
        name = "heater", input = "temperature", setpoint = 500,
        kp = 0.1, output_min = 0, output_max = 70,
        ambient_temperature = 20, max_power = 2500, heater_lag = 90,
        linear_loss = 0.35, radiation_loss_1000c = 1200, safe_output = 0,
    }"#,
    )
    .eval()
    .unwrap()
}

fn parameter() -> VirtualParameterDescriptor {
    VirtualParameterDescriptor::new(
        VirtualParameterId::new(1),
        "heater_power",
        "Heater power",
        ParameterAccess::ReadWrite,
        ParameterValueType::Number,
    )
}

#[test]
fn furnace_constructor_works_on_both_instrument_handles() {
    let lua = Lua::new();
    let (sender, receiver) = unbounded();
    lua.globals()
        .set(
            "plant",
            LuaVirtualInstrument {
                connection_id: ConnectionId::new(3),
                id: 1,
                descriptor: VirtualInstrumentDescriptor::new(
                    VirtualInstrumentId::new(1),
                    "Furnace",
                    vec![parameter()],
                )
                .unwrap(),
                command_sender: sender.clone(),
            },
        )
        .unwrap();
    lua.globals()
        .set(
            "metakon",
            LuaMetakon5x3 {
                connection_id: ConnectionId::new(3),
                instrument: Metakon5x3::new(1, 0),
                scale: 1.0,
                command_sender: sender,
            },
        )
        .unwrap();
    lua.globals().set("options", options(&lua)).unwrap();
    lua.load("plant:furnace('heater_power', options); metakon:furnace('output_power', options)")
        .exec()
        .unwrap();
    for _ in 0..2 {
        let UserCommand::Controller(ControllerCommand::Add(request)) = receiver.try_recv().unwrap()
        else {
            panic!("expected controller registration");
        };
        assert_eq!(request.name(), "heater");
        assert_eq!(request.input_name(), "temperature");
        assert_eq!(request.controller().kind(), ControllerKind::Furnace);
        assert_eq!(
            request.output_target().connection_id(),
            ConnectionId::new(3)
        );
        assert!(
            request
                .output_target()
                .safe_write_request()
                .unwrap()
                .is_some()
        );
        for (key, value) in [
            ("setpoint", 500.0),
            ("kp", 0.1),
            ("ki", 0.0),
            ("output_min", 0.0),
            ("output_max", 70.0),
            ("ambient_temperature", 20.0),
            ("max_power", 2500.0),
            ("heater_lag", 90.0),
            ("linear_loss", 0.35),
            ("radiation_loss_1000c", 1200.0),
        ] {
            assert_eq!(
                request.controller().read(key),
                Ok(InstrumentValue::Number(value))
            );
        }
    }
    assert!(receiver.is_empty());
    assert!(
        lua.load("plant:furnace('missing', options)")
            .exec()
            .is_err()
    );
    assert!(receiver.is_empty());
}

#[test]
fn furnace_constructor_rejects_invalid_options_without_sending_commands() {
    let lua = Lua::new();
    let (sender, receiver) = unbounded();
    for change in [
        "o.max_power = nil",
        "o.name = nil",
        "o.input = ''",
        "o.kd = 1",
        "o.thermal_capacity = 12000",
        "o.kp = -1",
        "o.max_power = 0",
        "o.heater_lag = -1",
        "o.ambient_temperature = -300",
        "o.linear_loss = -1",
        "o.radiation_loss_1000c = -1",
        "o.output_min = 80",
        "o.setpoint = 0/0",
    ] {
        let options = options(&lua);
        lua.globals().set("o", options.clone()).unwrap();
        lua.load(change).exec().unwrap();
        let target = ControlOutputTarget::virtual_instrument(
            ConnectionId::PRIMARY,
            VirtualInstrumentId::new(1),
            &parameter(),
        )
        .unwrap();
        assert!(
            add_furnace_loop(&sender, target, &options).is_err(),
            "{change}"
        );
        assert!(receiver.is_empty(), "{change}");
    }
}

#[test]
fn furnace_diagnostics_reach_processing_series() {
    let lua = Lua::new();
    let (sender, receiver) = unbounded();
    let target = ControlOutputTarget::virtual_instrument(
        ConnectionId::PRIMARY,
        VirtualInstrumentId::new(1),
        &parameter(),
    )
    .unwrap();
    add_furnace_loop(&sender, target, &options(&lua)).unwrap();
    let UserCommand::Controller(ControllerCommand::Add(request)) = receiver.try_recv().unwrap()
    else {
        panic!("expected controller");
    };
    let (name, _, target, controller) = request.into_parts();
    let service = ProcessingService::<u64>::spawn().unwrap();
    let handle = service.handle();
    handle
        .add_control_loop(ControlLoopDefinition::new(name, 1, target, controller).unwrap())
        .unwrap();
    for (key, id) in [
        ("feed_forward", 2),
        ("predicted_measurement", 3),
        ("measurement_rate", 4),
    ] {
        handle
            .add_controller_diagnostic(
                "heater".to_owned(),
                ControllerDiagnostic::from_key(key).unwrap(),
                id,
            )
            .unwrap();
    }
    let outputs = service.control_event_receiver();
    let samples = service.event_receiver();
    for (timestamp, measurement) in [(0.0, 20.0), (1.0, 22.0)] {
        handle.process(1, timestamp, measurement).unwrap();
        let ControlEvent::Output(event) = outputs.recv_timeout(Duration::from_secs(1)).unwrap()
        else {
            panic!("expected output");
        };
        let ControllerOutput::Furnace { output, .. } = event.output else {
            panic!("expected furnace output");
        };
        let ProcessingEvent::Samples(values) =
            samples.recv_timeout(Duration::from_secs(1)).unwrap()
        else {
            panic!("expected diagnostics");
        };
        assert_eq!(values.len(), 3);
        for (id, expected) in [
            (2, output.feed_forward()),
            (3, output.predicted_measurement()),
            (4, output.measurement_rate()),
        ] {
            let sample = values.iter().find(|sample| sample.signal_id == id).unwrap();
            assert_eq!(sample.timestamp, timestamp);
            assert_eq!(sample.value, expected);
        }
        if timestamp == 1.0 {
            assert!(output.measurement_rate() > 0.0 && output.measurement_rate() < 2.0);
            assert_eq!(
                output.predicted_measurement(),
                measurement + 90.0 * output.measurement_rate()
            );
        }
    }
}

#[test]
fn furnace_demo_switches_modes_and_handles_failures() {
    let lua = Lua::new();
    lua.load(include_str!(
        "../../lua_scripts/tests/furnace_manual_demo.lua"
    ))
    .exec()
    .unwrap();
    let app: Table = lua.globals().get("app").unwrap();
    let mock_register: mlua::Function = app.get("register_script").unwrap();
    let (events, _event_receiver) = unbounded();
    crate::lua_application_script::install(&lua, &app, events).unwrap();
    let real_register: mlua::Function = app.get("register_script").unwrap();
    app.set(
        "register_script",
        lua.create_function(move |_, script: Table| {
            real_register.call::<()>(script.clone())?;
            mock_register.call::<()>(script)
        })
        .unwrap(),
    )
    .unwrap();
    lua.load(include_str!("../../lua_scripts/furnace_manual_demo.lua"))
        .exec()
        .unwrap();
    lua.load("test_furnace_demo()").exec().unwrap();
}
