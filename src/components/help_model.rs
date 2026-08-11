#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HelpLanguage {
    #[default]
    English,
    Russian,
}

#[derive(Default)]
pub struct HelpModel {
    command_reference_open: bool,
    language: HelpLanguage,
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

    pub const fn language(&self) -> HelpLanguage {
        self.language
    }

    pub fn set_language(&mut self, language: HelpLanguage) {
        self.language = language;
    }
}
