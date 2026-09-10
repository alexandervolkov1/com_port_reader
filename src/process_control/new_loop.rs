use std::{error::Error, fmt};

use super::Controller;

#[derive(Debug)]
pub struct NewController<OutputTarget> {
    name: String,
    input_name: String,
    output_target: OutputTarget,
    controller: Controller,
}

impl<OutputTarget> NewController<OutputTarget> {
    pub fn new(
        name: impl Into<String>,
        input_name: impl Into<String>,
        output_target: OutputTarget,
        controller: impl Into<Controller>,
    ) -> Result<Self, NewControllerError> {
        let name = name.into();
        let input_name = input_name.into();

        if name.trim().is_empty() {
            return Err(NewControllerError::new("Controller name cannot be empty"));
        }

        if input_name.trim().is_empty() {
            return Err(NewControllerError::new(
                "Controller input series name cannot be empty",
            ));
        }

        Ok(Self {
            name,
            input_name,
            output_target,
            controller: controller.into(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn input_name(&self) -> &str {
        &self.input_name
    }

    pub const fn output_target(&self) -> &OutputTarget {
        &self.output_target
    }

    pub const fn controller(&self) -> &Controller {
        &self.controller
    }

    pub fn into_parts(self) -> (String, String, OutputTarget, Controller) {
        (
            self.name,
            self.input_name,
            self.output_target,
            self.controller,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewControllerError {
    message: String,
}

impl NewControllerError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for NewControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for NewControllerError {}

#[cfg(test)]
mod tests {
    use super::NewController;
    use crate::process_control::{ControllerKind, OnOffController};

    fn controller() -> OnOffController {
        OnOffController::new(150.0, 2.0, 0.0, 100.0).unwrap()
    }

    #[test]
    fn stores_generic_controller_request() {
        let request =
            NewController::new("thermostat", "temperature_filtered", 17_u64, controller()).unwrap();

        assert_eq!(request.name(), "thermostat",);

        assert_eq!(request.input_name(), "temperature_filtered",);

        assert_eq!(request.output_target(), &17,);

        assert_eq!(request.controller().kind(), ControllerKind::OnOff,);

        let (name, input_name, output_target, controller) = request.into_parts();

        assert_eq!(name, "thermostat",);

        assert_eq!(input_name, "temperature_filtered",);

        assert_eq!(output_target, 17,);

        assert_eq!(controller.kind(), ControllerKind::OnOff,);
    }

    #[test]
    fn rejects_empty_controller_name() {
        let error = NewController::new("", "temperature", (), controller()).unwrap_err();

        assert_eq!(error.to_string(), "Controller name cannot be empty",);
    }

    #[test]
    fn rejects_empty_input_name() {
        let error = NewController::new("thermostat", "", (), controller()).unwrap_err();

        assert_eq!(
            error.to_string(),
            "Controller input series name cannot be empty",
        );
    }
}
