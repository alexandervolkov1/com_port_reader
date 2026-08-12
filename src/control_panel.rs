use std::{collections::HashSet, error::Error, fmt};

#[derive(Clone, Debug, PartialEq)]
pub struct ControlPanelDefinition {
    id: String,
    title: String,
    controls: Vec<ControlDefinition>,
}

impl ControlPanelDefinition {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        controls: Vec<ControlDefinition>,
    ) -> Result<Self, ControlPanelDefinitionError> {
        let definition = Self {
            id: id.into(),
            title: title.into(),
            controls,
        };

        definition.validate()?;

        Ok(definition)
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn controls(&self) -> &[ControlDefinition] {
        &self.controls
    }

    fn validate(&self) -> Result<(), ControlPanelDefinitionError> {
        validate_identifier("panel", &self.id)?;

        if self.title.trim().is_empty() {
            return Err(ControlPanelDefinitionError::new(
                "Control panel title cannot be empty",
            ));
        }

        let mut control_ids = HashSet::new();

        for control in &self.controls {
            control.validate()?;

            if !control_ids.insert(control.id()) {
                return Err(ControlPanelDefinitionError::new(format!(
                    "Control panel '{}' contains duplicate control id '{}'",
                    self.id,
                    control.id(),
                )));
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControlDefinition {
    Readout {
        id: String,
        label: String,
        initial_text: String,
    },

    Number {
        id: String,
        label: String,
        initial_value: f64,
        minimum: Option<f64>,
        maximum: Option<f64>,
        step: f64,
        on_change: String,
    },

    Toggle {
        id: String,
        label: String,
        initial_value: bool,
        on_change: String,
    },

    Button {
        id: String,
        label: String,
        on_click: String,
    },
}

impl ControlDefinition {
    pub fn id(&self) -> &str {
        match self {
            Self::Readout { id, .. }
            | Self::Number { id, .. }
            | Self::Toggle { id, .. }
            | Self::Button { id, .. } => id,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Readout { label, .. }
            | Self::Number { label, .. }
            | Self::Toggle { label, .. }
            | Self::Button { label, .. } => label,
        }
    }

    fn validate(&self) -> Result<(), ControlPanelDefinitionError> {
        validate_identifier("control", self.id())?;

        if self.label().trim().is_empty() {
            return Err(ControlPanelDefinitionError::new(format!(
                "Control '{}' label cannot be empty",
                self.id(),
            )));
        }

        match self {
            Self::Readout { .. } => Ok(()),

            Self::Number {
                id,
                initial_value,
                minimum,
                maximum,
                step,
                on_change,
                ..
            } => {
                validate_finite_number(id, "initial value", *initial_value)?;
                validate_optional_number(id, "minimum", *minimum)?;
                validate_optional_number(id, "maximum", *maximum)?;
                validate_finite_number(id, "step", *step)?;

                if *step <= 0.0 {
                    return Err(ControlPanelDefinitionError::new(format!(
                        "Number control '{id}' step must be greater than zero",
                    )));
                }

                if let (Some(minimum), Some(maximum)) = (minimum, maximum)
                    && minimum > maximum
                {
                    return Err(ControlPanelDefinitionError::new(format!(
                        "Number control '{id}' minimum cannot exceed maximum",
                    )));
                }

                if let Some(minimum) = minimum
                    && initial_value < minimum
                {
                    return Err(ControlPanelDefinitionError::new(format!(
                        "Number control '{id}' initial value is below its minimum",
                    )));
                }

                if let Some(maximum) = maximum
                    && initial_value > maximum
                {
                    return Err(ControlPanelDefinitionError::new(format!(
                        "Number control '{id}' initial value exceeds its maximum",
                    )));
                }

                validate_callback(id, "on_change", on_change)
            }

            Self::Toggle { id, on_change, .. } => validate_callback(id, "on_change", on_change),

            Self::Button { id, on_click, .. } => validate_callback(id, "on_click", on_click),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlPanelDefinitionError {
    message: String,
}

impl ControlPanelDefinitionError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ControlPanelDefinitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ControlPanelDefinitionError {}

fn validate_identifier(kind: &str, identifier: &str) -> Result<(), ControlPanelDefinitionError> {
    if identifier.trim().is_empty() {
        return Err(ControlPanelDefinitionError::new(format!(
            "{kind} id cannot be empty",
        )));
    }

    let mut characters = identifier.chars();

    let valid_first_character = characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_');

    let remaining_characters_are_valid =
        characters.all(|character| character.is_ascii_alphanumeric() || character == '_');

    if !valid_first_character || !remaining_characters_are_valid {
        return Err(ControlPanelDefinitionError::new(format!(
            "Invalid {kind} id '{identifier}': \
             use an ASCII letter or underscore first, followed by letters, digits or underscores",
        )));
    }

    Ok(())
}

fn validate_callback(
    control_id: &str,
    field_name: &str,
    callback: &str,
) -> Result<(), ControlPanelDefinitionError> {
    if callback.trim().is_empty() {
        return Err(ControlPanelDefinitionError::new(format!(
            "Control '{control_id}' {field_name} callback cannot be empty",
        )));
    }

    validate_identifier("Lua callback", callback)
}

fn validate_optional_number(
    control_id: &str,
    field_name: &str,
    value: Option<f64>,
) -> Result<(), ControlPanelDefinitionError> {
    if let Some(value) = value {
        validate_finite_number(control_id, field_name, value)?;
    }

    Ok(())
}

fn validate_finite_number(
    control_id: &str,
    field_name: &str,
    value: f64,
) -> Result<(), ControlPanelDefinitionError> {
    if !value.is_finite() {
        return Err(ControlPanelDefinitionError::new(format!(
            "Number control '{control_id}' {field_name} must be finite",
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_control_panel() {
        let panel = ControlPanelDefinition::new(
            "metakon",
            "Metakon 5X3",
            vec![
                ControlDefinition::Readout {
                    id: "temperature".to_owned(),
                    label: "Temperature".to_owned(),
                    initial_text: "—".to_owned(),
                },
                ControlDefinition::Number {
                    id: "setpoint".to_owned(),
                    label: "Setpoint".to_owned(),
                    initial_value: 150.0,
                    minimum: Some(0.0),
                    maximum: Some(1_000.0),
                    step: 1.0,
                    on_change: "set_metakon_setpoint".to_owned(),
                },
                ControlDefinition::Toggle {
                    id: "automatic".to_owned(),
                    label: "Automatic mode".to_owned(),
                    initial_value: true,
                    on_change: "set_metakon_automatic".to_owned(),
                },
                ControlDefinition::Button {
                    id: "stop_heating".to_owned(),
                    label: "Stop heating".to_owned(),
                    on_click: "stop_heating".to_owned(),
                },
            ],
        )
        .unwrap();

        assert_eq!(panel.id(), "metakon");
        assert_eq!(panel.title(), "Metakon 5X3");
        assert_eq!(panel.controls().len(), 4);
    }

    #[test]
    fn rejects_duplicate_control_ids() {
        let result = ControlPanelDefinition::new(
            "controller",
            "Controller",
            vec![
                ControlDefinition::Readout {
                    id: "temperature".to_owned(),
                    label: "Temperature".to_owned(),
                    initial_text: "—".to_owned(),
                },
                ControlDefinition::Readout {
                    id: "temperature".to_owned(),
                    label: "Temperature again".to_owned(),
                    initial_text: "—".to_owned(),
                },
            ],
        );

        assert_eq!(
            result.unwrap_err().to_string(),
            "Control panel 'controller' contains duplicate control id 'temperature'",
        );
    }

    #[test]
    fn rejects_invalid_number_range() {
        let result = ControlPanelDefinition::new(
            "controller",
            "Controller",
            vec![ControlDefinition::Number {
                id: "setpoint".to_owned(),
                label: "Setpoint".to_owned(),
                initial_value: 150.0,
                minimum: Some(200.0),
                maximum: Some(100.0),
                step: 1.0,
                on_change: "set_setpoint".to_owned(),
            }],
        );

        assert_eq!(
            result.unwrap_err().to_string(),
            "Number control 'setpoint' minimum cannot exceed maximum",
        );
    }

    #[test]
    fn rejects_invalid_identifier() {
        let result = ControlPanelDefinition::new("Metakon panel", "Metakon", Vec::new());

        assert!(result.unwrap_err().to_string().contains("Invalid panel id"));
    }

    #[test]
    fn rejects_empty_callback() {
        let result = ControlPanelDefinition::new(
            "controller",
            "Controller",
            vec![ControlDefinition::Button {
                id: "stop".to_owned(),
                label: "Stop".to_owned(),
                on_click: String::new(),
            }],
        );

        assert_eq!(
            result.unwrap_err().to_string(),
            "Control 'stop' on_click callback cannot be empty",
        );
    }
}
