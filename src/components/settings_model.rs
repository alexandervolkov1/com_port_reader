use std::path::{Path, PathBuf};

use crate::application_runtime::ApplicationRuntime;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SettingsValidation {
    #[default]
    NotChecked,
    Valid,
    Invalid(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SettingsReloadStatus {
    #[default]
    NotReloaded,
    Succeeded,
    Failed(String),
}

#[derive(Default)]
pub struct SettingsModel {
    open: bool,
    selected_profile: Option<PathBuf>,
    validation: SettingsValidation,
    open_error: Option<String>,
    reload_confirmation_open: bool,
    reload_requested: bool,
    reload_status: SettingsReloadStatus,
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

    pub fn selected_profile(&self) -> Option<&Path> {
        self.selected_profile.as_deref()
    }

    pub fn select_profile(&mut self, path: PathBuf) {
        self.selected_profile = Some(path);
        self.validation = SettingsValidation::NotChecked;
        self.reload_status = SettingsReloadStatus::NotReloaded;
        self.open_error = None;
    }

    pub fn clear_selected_profile(&mut self) {
        self.selected_profile = None;
        self.validation = SettingsValidation::NotChecked;
        self.reload_status = SettingsReloadStatus::NotReloaded;
    }

    pub const fn validation(&self) -> &SettingsValidation {
        &self.validation
    }

    pub fn validate(&mut self, runtime: &ApplicationRuntime) {
        let result = match self.selected_profile() {
            Some(path) => runtime.validate_profile_configuration(path),
            None => runtime.validate_startup_configuration(),
        };

        self.validation = match result {
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

    pub const fn reload_confirmation_open(&self) -> bool {
        self.reload_confirmation_open
    }

    pub fn begin_reload_confirmation(&mut self) {
        self.reload_confirmation_open = true;
    }

    pub fn cancel_reload(&mut self) {
        self.reload_confirmation_open = false;
    }

    pub fn confirm_reload(&mut self) {
        self.reload_confirmation_open = false;
        self.reload_requested = true;
    }

    pub fn take_reload_request(&mut self) -> bool {
        std::mem::take(&mut self.reload_requested)
    }

    pub const fn reload_status(&self) -> &SettingsReloadStatus {
        &self.reload_status
    }

    pub fn set_reload_result(&mut self, result: Result<(), String>) {
        self.reload_status = match result {
            Ok(()) => {
                self.validation = SettingsValidation::Valid;

                SettingsReloadStatus::Succeeded
            }

            Err(error) => SettingsReloadStatus::Failed(error),
        };
    }
}
