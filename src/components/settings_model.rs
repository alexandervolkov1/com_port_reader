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
    open_error: Option<String>,
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

    pub fn open_startup_file(&mut self, runtime: &ApplicationRuntime) {
        self.open_error = runtime.open_startup_configuration().err();
    }

    pub fn open_error(&self) -> Option<&str> {
        self.open_error.as_deref()
    }
}
