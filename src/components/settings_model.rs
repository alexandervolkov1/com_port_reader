use crate::application_runtime::ApplicationRuntime;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsValidation {
    NotChecked,
    Valid,
    Invalid(String),
}

impl Default for SettingsValidation {
    fn default() -> Self {
        Self::NotChecked
    }
}

#[derive(Default)]
pub struct SettingsModel {
    open: bool,
    validation: SettingsValidation,
}

impl SettingsModel {
    pub const fn is_open(&self) -> bool {
        self.open
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub const fn validation(&self) -> &SettingsValidation {
        &self.validation
    }

    pub fn validate(&mut self, runtime: &ApplicationRuntime) {
        self.validation = match runtime.validate_startup_configuration() {
            Ok(()) => SettingsValidation::Valid,

            Err(error) => SettingsValidation::Invalid(error),
        };
    }
}
