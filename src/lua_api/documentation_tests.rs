//! Executable documentation checks: syntax is separate from runtime acceptance tests.

/// GitHub-style anchors for the simple unique headings used in the project guides.
fn heading_anchor(heading: &str) -> String {
    heading
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_alphanumeric() || matches!(ch, ' ' | '-' | '_'))
        .map(|ch| if ch == ' ' { '-' } else { ch })
        .collect()
}

#[test]
fn documentation_relative_links_and_anchors_resolve() {
    for path in documentation_files() {
        let source = fs::read_to_string(&path).unwrap();
        for part in source.split("](").skip(1) {
            let target = part.split(')').next().unwrap();
            if target.starts_with("https://") || target.starts_with("http://") {
                continue;
            }
            let (file, anchor) = target.split_once('#').unwrap_or((target, ""));
            let destination = if file.is_empty() {
                path.clone()
            } else {
                path.parent().unwrap().join(file)
            };
            assert!(
                destination.exists(),
                "{}: broken link {target}",
                path.display()
            );
            if !anchor.is_empty()
                && destination
                    .extension()
                    .is_some_and(|extension| extension == "md")
            {
                let linked = fs::read_to_string(&destination).unwrap();
                assert!(
                    linked
                        .lines()
                        .filter(|line| line.starts_with('#'))
                        .any(|line| heading_anchor(line.trim_start_matches('#').trim()) == anchor),
                    "{}: broken anchor {target}",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn documentation_inline_application_examples_compile() {
    let lua = Lua::new();
    for path in documentation_files() {
        let source = fs::read_to_string(&path).unwrap();
        for code in source.split('`').skip(1).step_by(2) {
            let candidate = code.trim().strip_prefix("return ").unwrap_or(code.trim());
            if candidate.starts_with("app.") && candidate.contains('(') && !candidate.contains('\n')
            {
                lua.load(code).into_function().unwrap_or_else(|error| {
                    panic!("{} inline example {code}: {error}", path.display())
                });
            }
        }
    }
}
use mlua::{Lua, Table, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Extract fenced blocks without executing them; fragments are compiled independently.
pub(crate) fn fenced_blocks<'a>(source: &'a str, language: &str) -> Vec<&'a str> {
    let marker = format!("```{language}\n");
    source
        .split(&marker)
        .skip(1)
        .map(|part| part.split("```").next().unwrap())
        .collect()
}

fn documentation_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![root.join("README.md")];
    files.extend(
        fs::read_dir(root.join("docs"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "md")),
    );
    files.sort();
    files
}

#[test]
fn documentation_lua_blocks_compile() {
    let lua = Lua::new();
    let mut count = 0;
    for path in documentation_files() {
        let source = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
        for (index, block) in fenced_blocks(&source, "lua").iter().enumerate() {
            lua.load(*block).into_function().unwrap_or_else(|error| {
                panic!("{} Lua block {}: {error}", path.display(), index + 1)
            });
            count += 1;
        }
    }
    assert!(count >= 50, "unexpectedly few Lua examples: {count}");
}

#[test]
fn documentation_and_editor_types_cover_registered_app_functions() {
    let lua = Lua::new();
    let (commands, _receiver) = crossbeam_channel::unbounded();
    let (events, _events_receiver) = crossbeam_channel::unbounded();
    super::install(&lua, commands, events, &Default::default()).unwrap();
    let app: Table = lua.globals().get("app").unwrap();
    let reference = include_str!("../../docs/lua-api.md");
    let declarations = include_str!("../../lua_types/app.d.lua");
    for entry in app.pairs::<String, Value>() {
        let (name, value) = entry.unwrap();
        assert!(
            matches!(value, Value::Function(_)),
            "unexpected app member: {name}"
        );
        assert!(
            reference.contains(&format!("app.{name}")),
            "undocumented app.{name}"
        );
        assert!(
            declarations.contains(&format!("function app.{name}(")),
            "missing editor type app.{name}"
        );
    }
}

#[test]
fn documentation_and_editor_types_cover_userdata_methods() {
    let reference = format!(
        "{}\n{}",
        include_str!("../../docs/lua-api.md"),
        include_str!("../../docs/scenarios.md")
    );
    let declarations = include_str!("../../lua_types/app.d.lua");
    for (source, class) in [
        (include_str!("controllers.rs"), "Controller"),
        (include_str!("metakon.rs"), "Metakon5x3"),
        (include_str!("virtual_instrument.rs"), "VirtualInstrument"),
        (include_str!("scenarios.rs"), "Scenario"),
    ] {
        let mut count = 0;
        // Userdata bindings use literal names, including multiline add_method calls.
        for call in source.split("methods.add_method").skip(1) {
            let name = call
                .split('"')
                .nth(1)
                .expect("literal userdata method name");
            assert!(
                reference.contains(&format!(":{name}(")),
                "{class}:{name} has no example"
            );
            assert!(
                declarations.contains(&format!("function {class}:{name}(")),
                "{class}:{name} missing editor type"
            );
            count += 1;
        }
        assert!(
            count >= 7,
            "{class}: did not discover userdata registrations"
        );
    }
}

#[test]
fn metakon_controller_editor_annotations_use_the_declared_parameter_alias() {
    let declarations = include_str!("../../lua_types/app.d.lua");
    assert!(declarations.contains("---@alias Metakon5x3Parameter"));
    for constructor in ["pid", "on_off", "furnace"] {
        let signature = format!("function Metakon5x3:{constructor}(");
        let prefix = declarations.split(&signature).next().unwrap();
        assert!(declarations.contains(&signature), "missing {signature}");
        let annotation = prefix.rsplit("---@param parameter ").next().unwrap();
        assert_eq!(
            annotation.split_whitespace().next(),
            Some("Metakon5x3Parameter"),
            "{constructor}: unknown parameter alias"
        );
    }
}

#[test]
fn documented_virtual_model_preserves_written_state() {
    use crate::{
        instrument::InstrumentValue, lua_virtual_instrument_model::LuaVirtualInstrumentModel,
        protocol::virtual_instrument::VirtualInstrumentModel,
    };
    let source = include_str!("../../docs/virtual-instruments.md");
    let block = fenced_blocks(source, "lua")[0];
    let mut model = LuaVirtualInstrumentModel::from_source(block).unwrap();
    let instrument = &model.instruments()[0];
    let id = instrument.id();
    let parameter = instrument.parameters()[0].id();
    let value = InstrumentValue::Number(42.0);
    assert_eq!(
        model
            .write(id, parameter, value, Default::default())
            .unwrap(),
        value
    );
    assert_eq!(
        model.read(id, parameter, Default::default()).unwrap(),
        value
    );
}

#[test]
fn sine_model_retuning_is_continuous_and_reaches_requested_amplitude() {
    let lua = Lua::new();
    lua.load(include_str!("../../emulator_scripts/sine_generator.lua"))
        .exec()
        .unwrap();
    lua.load(r#"
        local function near(a, b) assert(math.abs(a - b) < 1e-9, tostring(a) .. " ~= " .. tostring(b)) end
        write(1, "amplitude", 100, 0)
        write(1, "period", 10, 0)
        write(1, "phase", 0.4, 0)
        write(1, "transition_seconds", 2, 0)
        local before = read(1, "value", 3)
        write(1, "period", 1, 3)
        near(read(1, "value", 3), before)
        write(1, "amplitude", 10, 3)
        near(read(1, "value", 3), before)
        near(read(1, "amplitude", 3), 10) -- readback is the target, not the ramp.
        near(read(1, "value", 5), before / 10)
        write(1, "amplitude", 80, 5)
        before = read(1, "value", 5.7)
        write(1, "amplitude", 0, 5.7)
        near(read(1, "value", 5.7), before)
        near(read(1, "value", 7.7), 0)
        write(1, "noise_amplitude", 100, 7.7)
        near(read(1, "value", 7.7), 0)
        assert(math.abs(read(1, "value", 8.7)) <= 50.000001)
        write(1, "noise_amplitude", 0, 8.7)
        near(read(1, "value", 10.7), 0)
        for _, key in ipairs({"amplitude", "period", "noise_amplitude", "transition_seconds"}) do
            assert(not pcall(write, 1, key, -1, 11))
            assert(not pcall(write, 1, key, 0/0, 11))
        end
        -- A long-running generator keeps its angle across repeated period changes.
        write(2, "period", 13, 0)
        before = read(2, "value", 1000000)
        for _, period in ipairs({1, 300, 0.5, 80}) do
            write(2, "period", period, 1000000)
            near(read(2, "value", 1000000), before)
        end
    "#).exec().unwrap();
}

#[test]
fn sine_braid_controls_update_all_waves_and_restart_with_saved_settings() {
    let lua = Lua::new();
    lua.load(r#"
        local devices, filters, script = {}, {}, nil
        app = {
            stop = function() end, stop_emu = function() end,
            start = function() end, start_emu = function() end,
            clear = function() devices = {}; filters = {} end,
            unregister_script = function() end,
            register_script = function(value) script = value end,
            set_control = function() end, set_control_enabled = function() end,
            virtual_instrument = function(options)
                local device = {}
                function device:write(key, value) self[key] = value; return value end
                function device:add() end
                devices[options.id] = device
                return device
            end,
            filter = function(_, options) filters[options.name] = options.time_constant end,
            set_filter = function(name, options) assert(filters[name]); filters[name] = options.time_constant end,
            delete = function(name) filters[name] = nil end,
        }
        function verify_braid()
            assert(#script.panels == 2 and #devices == 8)
            script.set_amplitude(200)
            script.set_noise(15)
            script.set_period(40)
            script.set_filter_time_constant(12)
            local proportions = {1, .9, .8, .7, .7, .8, .9, 1}
            for i = 1, 8 do
                assert(devices[i].amplitude == 200 * proportions[i])
                assert(devices[i].noise_amplitude == 15 and devices[i].period == 40)
                assert(filters["braid_" .. i .. "_ema"] == 12)
                assert(devices[i].transition_seconds == 2)
            end
            script.set_filter_enabled_3(false)
            script.set_filter_time_constant(8)
            assert(not filters.braid_3_ema)
            script.set_filter_enabled_3(true)
            assert(filters.braid_3_ema == 8)
            script.stop()
            script.set_amplitude(50)
            script.run()
            for i = 1, 8 do
                assert(devices[i].amplitude == 50 * proportions[i])
                assert(filters["braid_" .. i .. "_ema"] == 8)
            end
        end
    "#).exec().unwrap();
    lua.load(include_str!("../../lua_scripts/sine_braid_demo.lua"))
        .exec()
        .unwrap();
    lua.load("verify_braid()").exec().unwrap();
}
