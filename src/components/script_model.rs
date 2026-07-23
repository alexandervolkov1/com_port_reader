use std::{fs, path::Path};

use crate::{
    app_log::LogHandle,
    components::{command_model::CommandModel, controls_model::ControlsModel},
    script::parse_script,
};

pub struct ScriptModel;

impl ScriptModel {
    pub const fn new() -> Self {
        Self
    }

    pub fn start_from_file(
        &mut self,
        path: &Path,
        commands: &mut CommandModel,
        controls: &ControlsModel,
        log: &LogHandle,
    ) {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,

            Err(error) => {
                log.error(format!(
                    "Failed to read script '{}': \
                         {error}",
                    path.display(),
                ));

                return;
            }
        };

        let script_commands = match parse_script(&contents) {
            Ok(commands) => commands,

            Err(error) => {
                log.error(format!(
                    "Failed to parse script '{}':\n\
                         {error}",
                    path.display(),
                ));

                return;
            }
        };

        let command_count = script_commands.len();

        for command in script_commands {
            commands.execute(command);
        }

        controls.start();

        log.info(format!(
            "Submitted {command_count} command(s) \
             from '{}' and requested acquisition \
             start.",
            path.display(),
        ));
    }
}

impl Default for ScriptModel {
    fn default() -> Self {
        Self::new()
    }
}
