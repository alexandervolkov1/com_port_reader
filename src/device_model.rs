use std::{path::PathBuf, time::Duration};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceModelSource {
    BuiltIn,
    LuaScript(PathBuf),
}

pub trait DeviceModel {
    fn handle_command(
        &mut self,
        command: &str,
        elapsed: Duration,
    ) -> Result<String, DeviceModelError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceModelError {
    message: String,
}

impl std::fmt::Display for DeviceModelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for DeviceModelError {}

impl From<String> for DeviceModelError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<&str> for DeviceModelError {
    fn from(message: &str) -> Self {
        Self {
            message: message.to_owned(),
        }
    }
}
