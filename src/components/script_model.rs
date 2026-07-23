use std::{fs, path::Path};

use crate::{
    components::{command_model::CommandModel, controls_model::ControlsModel},
    script::parse_script,
};

#[derive(Default)]
pub struct ScriptModel {
    message: Option<String>,
    has_error: bool,
}

impl ScriptModel {
    pub fn start_from_file(
        &mut self,
        path: &Path,
        commands: &mut CommandModel,
        controls: &ControlsModel,
    ) {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,

            Err(error) => {
                self.set_error(format!(
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
                self.set_error(format!(
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

        self.message = Some(format!(
            "Submitted {command_count} command(s) \
             from '{}' and requested acquisition start.",
            path.display(),
        ));

        self.has_error = false;
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub const fn has_error(&self) -> bool {
        self.has_error
    }

    fn set_error(&mut self, message: String) {
        self.message = Some(message);
        self.has_error = true;
    }
}
