use std::{error::Error, fmt};

use crate::control_panel::{ControlDefinition, ControlPanelDefinition};

#[derive(Clone, Debug, PartialEq)]
pub struct ControlPanelModel {
    panels: Vec<ControlPanelState>,
}

impl ControlPanelModel {
    pub fn new(definitions: &[ControlPanelDefinition]) -> Self {
        Self {
            panels: definitions.iter().map(ControlPanelState::from).collect(),
        }
    }

    pub fn panels(&self) -> &[ControlPanelState] {
        &self.panels
    }

    pub fn replace_definitions(&mut self, definitions: &[ControlPanelDefinition]) {
        self.panels = definitions.iter().map(ControlPanelState::from).collect();
    }

    pub fn set_control_value(
        &mut self,
        panel_id: &str,
        control_id: &str,
        value: ControlValue,
    ) -> Result<(), ControlPanelStateError> {
        let panel = self
            .panels
            .iter_mut()
            .find(|panel| panel.id() == panel_id)
            .ok_or_else(|| {
                ControlPanelStateError::new(format!("Control panel '{panel_id}' was not found",))
            })?;

        panel.set_control_value(control_id, value)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControlPanelState {
    id: String,
    title: String,
    controls: Vec<ControlState>,
}

impl ControlPanelState {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn controls(&self) -> &[ControlState] {
        &self.controls
    }

    fn set_control_value(
        &mut self,
        control_id: &str,
        value: ControlValue,
    ) -> Result<(), ControlPanelStateError> {
        let control = self
            .controls
            .iter_mut()
            .find(|control| control.id() == control_id)
            .ok_or_else(|| {
                ControlPanelStateError::new(format!(
                    "Control '{control_id}' was not found \
                     in panel '{}'",
                    self.id,
                ))
            })?;

        control.set_value(value)
    }
}

impl From<&ControlPanelDefinition> for ControlPanelState {
    fn from(definition: &ControlPanelDefinition) -> Self {
        Self {
            id: definition.id().to_owned(),
            title: definition.title().to_owned(),
            controls: definition
                .controls()
                .iter()
                .map(ControlState::from)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControlState {
    Readout {
        id: String,
        label: String,
        text: String,
    },

    Number {
        id: String,
        label: String,
        value: f64,
        minimum: Option<f64>,
        maximum: Option<f64>,
        step: f64,
        on_change: String,
    },

    Toggle {
        id: String,
        label: String,
        value: bool,
        on_change: String,
    },

    Button {
        id: String,
        label: String,
        on_click: String,
    },
}

impl ControlState {
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

    pub fn value(&self) -> Option<ControlValueRef<'_>> {
        match self {
            Self::Readout { text, .. } => Some(ControlValueRef::Text(text)),

            Self::Number { value, .. } => Some(ControlValueRef::Number(*value)),

            Self::Toggle { value, .. } => Some(ControlValueRef::Boolean(*value)),

            Self::Button { .. } => None,
        }
    }

    pub fn callback(&self) -> Option<&str> {
        match self {
            Self::Readout { .. } => None,

            Self::Number { on_change, .. } | Self::Toggle { on_change, .. } => Some(on_change),

            Self::Button { on_click, .. } => Some(on_click),
        }
    }

    fn set_value(&mut self, value: ControlValue) -> Result<(), ControlPanelStateError> {
        let actual_type = value.type_name();

        match (self, value) {
            (Self::Readout { text, .. }, ControlValue::Text(value)) => {
                *text = value;

                Ok(())
            }

            (
                Self::Number {
                    id,
                    value,
                    minimum,
                    maximum,
                    ..
                },
                ControlValue::Number(new_value),
            ) => {
                validate_number_value(id, new_value, *minimum, *maximum)?;

                *value = new_value;

                Ok(())
            }

            (Self::Toggle { value, .. }, ControlValue::Boolean(new_value)) => {
                *value = new_value;

                Ok(())
            }

            (control, _) => Err(ControlPanelStateError::new(format!(
                "Cannot assign {actual_type} to {} control '{}'",
                control.value_type_name(),
                control.id(),
            ))),
        }
    }

    fn value_type_name(&self) -> &'static str {
        match self {
            Self::Readout { .. } => "text",
            Self::Number { .. } => "number",
            Self::Toggle { .. } => "boolean",
            Self::Button { .. } => "button",
        }
    }
}

impl From<&ControlDefinition> for ControlState {
    fn from(definition: &ControlDefinition) -> Self {
        match definition {
            ControlDefinition::Readout {
                id,
                label,
                initial_text,
            } => Self::Readout {
                id: id.clone(),
                label: label.clone(),
                text: initial_text.clone(),
            },

            ControlDefinition::Number {
                id,
                label,
                initial_value,
                minimum,
                maximum,
                step,
                on_change,
            } => Self::Number {
                id: id.clone(),
                label: label.clone(),
                value: *initial_value,
                minimum: *minimum,
                maximum: *maximum,
                step: *step,
                on_change: on_change.clone(),
            },

            ControlDefinition::Toggle {
                id,
                label,
                initial_value,
                on_change,
            } => Self::Toggle {
                id: id.clone(),
                label: label.clone(),
                value: *initial_value,
                on_change: on_change.clone(),
            },

            ControlDefinition::Button {
                id,
                label,
                on_click,
            } => Self::Button {
                id: id.clone(),
                label: label.clone(),
                on_click: on_click.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControlValue {
    Text(String),
    Number(f64),
    Boolean(bool),
}

impl ControlValue {
    fn type_name(&self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Number(_) => "number",
            Self::Boolean(_) => "boolean",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControlValueRef<'a> {
    Text(&'a str),
    Number(f64),
    Boolean(bool),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlPanelStateError {
    message: String,
}

impl ControlPanelStateError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ControlPanelStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ControlPanelStateError {}

fn validate_number_value(
    control_id: &str,
    value: f64,
    minimum: Option<f64>,
    maximum: Option<f64>,
) -> Result<(), ControlPanelStateError> {
    if !value.is_finite() {
        return Err(ControlPanelStateError::new(format!(
            "Number control '{control_id}' value must be finite",
        )));
    }

    if let Some(minimum) = minimum
        && value < minimum
    {
        return Err(ControlPanelStateError::new(format!(
            "Number control '{control_id}' value {value} \
             is below minimum {minimum}",
        )));
    }

    if let Some(maximum) = maximum
        && value > maximum
    {
        return Err(ControlPanelStateError::new(format!(
            "Number control '{control_id}' value {value} \
             exceeds maximum {maximum}",
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_panel::{ControlDefinition, ControlPanelDefinition};

    fn definition() -> ControlPanelDefinition {
        ControlPanelDefinition::new(
            "metakon",
            "Metakon 5X3",
            vec![
                ControlDefinition::Readout {
                    id: "temperature".to_owned(),
                    label: "Temperature".to_owned(),
                    initial_text: "Waiting".to_owned(),
                },
                ControlDefinition::Number {
                    id: "setpoint".to_owned(),
                    label: "Setpoint".to_owned(),
                    initial_value: 150.0,
                    minimum: Some(0.0),
                    maximum: Some(1_000.0),
                    step: 1.0,
                    on_change: "set_setpoint".to_owned(),
                },
                ControlDefinition::Toggle {
                    id: "automatic".to_owned(),
                    label: "Automatic".to_owned(),
                    initial_value: true,
                    on_change: "set_automatic".to_owned(),
                },
                ControlDefinition::Button {
                    id: "stop".to_owned(),
                    label: "Stop heating".to_owned(),
                    on_click: "stop_heating".to_owned(),
                },
            ],
        )
        .unwrap()
    }

    #[test]
    fn initializes_state_from_definition() {
        let definition = definition();

        let model = ControlPanelModel::new(&[definition]);

        assert_eq!(model.panels().len(), 1);

        let panel = &model.panels()[0];

        assert_eq!(panel.id(), "metakon");
        assert_eq!(panel.title(), "Metakon 5X3");
        assert_eq!(panel.controls().len(), 4);

        assert_eq!(
            panel.controls()[0].value(),
            Some(ControlValueRef::Text("Waiting")),
        );

        assert_eq!(
            panel.controls()[1].value(),
            Some(ControlValueRef::Number(150.0)),
        );

        assert_eq!(
            panel.controls()[2].value(),
            Some(ControlValueRef::Boolean(true)),
        );

        assert_eq!(panel.controls()[3].value(), None);
    }

    #[test]
    fn changes_control_values() {
        let definition = definition();

        let mut model = ControlPanelModel::new(&[definition]);

        model
            .set_control_value(
                "metakon",
                "temperature",
                ControlValue::Text("201.5 °C".to_owned()),
            )
            .unwrap();

        model
            .set_control_value("metakon", "setpoint", ControlValue::Number(200.0))
            .unwrap();

        model
            .set_control_value("metakon", "automatic", ControlValue::Boolean(false))
            .unwrap();

        let controls = model.panels()[0].controls();

        assert_eq!(controls[0].value(), Some(ControlValueRef::Text("201.5 °C")),);

        assert_eq!(controls[1].value(), Some(ControlValueRef::Number(200.0)),);

        assert_eq!(controls[2].value(), Some(ControlValueRef::Boolean(false)),);
    }

    #[test]
    fn rejects_wrong_value_type() {
        let definition = definition();

        let mut model = ControlPanelModel::new(&[definition]);

        let error = model
            .set_control_value(
                "metakon",
                "setpoint",
                ControlValue::Text("two hundred".to_owned()),
            )
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Cannot assign text to number control 'setpoint'",
        );
    }

    #[test]
    fn rejects_number_outside_range() {
        let definition = definition();

        let mut model = ControlPanelModel::new(&[definition]);

        let error = model
            .set_control_value("metakon", "setpoint", ControlValue::Number(1_500.0))
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Number control 'setpoint' value 1500 exceeds maximum 1000",
        );
    }

    #[test]
    fn replaces_definitions_and_resets_state() {
        let first = definition();

        let second = ControlPanelDefinition::new(
            "vacuum",
            "Vacuum system",
            vec![ControlDefinition::Readout {
                id: "pressure".to_owned(),
                label: "Pressure".to_owned(),
                initial_text: "—".to_owned(),
            }],
        )
        .unwrap();

        let mut model = ControlPanelModel::new(&[first]);

        model.replace_definitions(&[second]);

        assert_eq!(model.panels().len(), 1);
        assert_eq!(model.panels()[0].id(), "vacuum");
        assert_eq!(model.panels()[0].controls().len(), 1);
    }
}
