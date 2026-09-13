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
