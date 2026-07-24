#[derive(Default)]
pub struct HelpModel {
    command_reference_open: bool,
}

impl HelpModel {
    pub fn open_command_reference(&mut self) {
        self.command_reference_open = true;
    }

    pub fn command_reference_open(&self) -> bool {
        self.command_reference_open
    }

    pub fn set_command_reference_open(&mut self, open: bool) {
        self.command_reference_open = open;
    }
}
