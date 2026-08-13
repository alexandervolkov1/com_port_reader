use std::{error::Error, fmt};

use crate::control_panel::{ControlDefinition, ControlPanelDefinition};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ControlPanelModel {
    panels: Vec<ControlPanelState>,
}

impl ControlPanelModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn panels(&self) -> &[ControlPanelState] {
        &self.panels
    }

    pub fn panels_mut(&mut self) -> &mut [ControlPanelState] {
        &mut self.panels
    }

    pub fn register_script(&mut self, script_id: &str, definitions: &[ControlPanelDefinition]) {
        self.unregister_script(script_id);

        self.panels.extend(
            definitions
                .iter()
                .map(|definition| ControlPanelState::from_definition(script_id, definition)),
        );
    }

    pub fn unregister_script(&mut self, script_id: &str) {
        self.panels.retain(|panel| panel.script_id() != script_id);
    }

    pub fn clear(&mut self) {
        self.panels.clear();
    }

    pub fn set_control_value(
        &mut self,
        script_id: &str,
        panel_id: &str,
        control_id: &str,
        value: ControlValue,
    ) -> Result<(), ControlPanelStateError> {
        let panel = self
            .panels
            .iter_mut()
            .find(|panel| panel.script_id() == script_id && panel.id() == panel_id)
            .ok_or_else(|| {
                ControlPanelStateError::new(format!(
                    "Control panel '{panel_id}' \
                     of application script \
                     '{script_id}' was not found",
                ))
            })?;

        panel.set_control_value(control_id, value)
    }

    pub fn discard_control_edit(
        &mut self,
        script_id: &str,
        panel_id: &str,
        control_id: &str,
    ) -> Result<(), ControlPanelStateError> {
        let panel = self
            .panels
            .iter_mut()
            .find(|panel| panel.script_id() == script_id && panel.id() == panel_id)
            .ok_or_else(|| {
                ControlPanelStateError::new(format!(
                    "Control panel '{panel_id}' \
                     of application script \
                     '{script_id}' was not found",
                ))
            })?;

        panel.discard_control_edit(control_id)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControlPanelState {
    id: String,
    title: String,
    controls: Vec<ControlState>,
    script_id: String,
}

impl ControlPanelState {
    fn from_definition(script_id: &str, definition: &ControlPanelDefinition) -> Self {
        Self {
            script_id: script_id.to_owned(),
            id: definition.id().to_owned(),
            title: definition.title().to_owned(),
            controls: definition
                .controls()
                .iter()
                .map(ControlState::from)
                .collect(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn controls(&self) -> &[ControlState] {
        &self.controls
    }

    pub fn controls_mut(&mut self) -> &mut [ControlState] {
        &mut self.controls
    }

    pub fn script_id(&self) -> &str {
        &self.script_id
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

    fn discard_control_edit(&mut self, control_id: &str) -> Result<(), ControlPanelStateError> {
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

        control.discard_edit();

        Ok(())
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
        draft_value: f64,
        minimum: Option<f64>,
        maximum: Option<f64>,
        step: f64,
        on_change: String,
    },

    Toggle {
        id: String,
        label: String,
        value: bool,
        draft_value: bool,
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

            Self::Number { draft_value, .. } => Some(ControlValueRef::Number(*draft_value)),

            Self::Toggle { draft_value, .. } => Some(ControlValueRef::Boolean(*draft_value)),

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
                    draft_value,
                    minimum,
                    maximum,
                    ..
                },
                ControlValue::Number(new_value),
            ) => {
                validate_number_value(id, new_value, *minimum, *maximum)?;

                *value = new_value;
                *draft_value = new_value;

                Ok(())
            }

            (
                Self::Toggle {
                    value, draft_value, ..
                },
                ControlValue::Boolean(new_value),
            ) => {
                *value = new_value;
                *draft_value = new_value;

                Ok(())
            }

            (control, _) => Err(ControlPanelStateError::new(format!(
                "Cannot assign {actual_type} to {} control '{}'",
                control.value_type_name(),
                control.id(),
            ))),
        }
    }

    fn discard_edit(&mut self) {
        match self {
            Self::Number {
                value, draft_value, ..
            } => {
                *draft_value = *value;
            }

            Self::Toggle {
                value, draft_value, ..
            } => {
                *draft_value = *value;
            }

            Self::Readout { .. } | Self::Button { .. } => {}
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
                draft_value: *initial_value,
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
                draft_value: *initial_value,
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

    fn model() -> ControlPanelModel {
        let definition = definition();

        let mut model = ControlPanelModel::new();

        model.register_script("metakon_script", &[definition]);

        model
    }

    #[test]
    fn initializes_state_from_definition() {
        let model = model();

        assert_eq!(model.panels().len(), 1);

        let panel = &model.panels()[0];

        assert_eq!(panel.script_id(), "metakon_script",);

        assert_eq!(panel.id(), "metakon",);

        assert_eq!(panel.title(), "Metakon 5X3",);

        assert_eq!(panel.controls().len(), 4,);

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

        assert_eq!(panel.controls()[3].value(), None,);
    }

    #[test]
    fn changes_control_values() {
        let mut model = model();

        model
            .set_control_value(
                "metakon_script",
                "metakon",
                "temperature",
                ControlValue::Text("201.5 °C".to_owned()),
            )
            .unwrap();

        model
            .set_control_value(
                "metakon_script",
                "metakon",
                "setpoint",
                ControlValue::Number(200.0),
            )
            .unwrap();

        model
            .set_control_value(
                "metakon_script",
                "metakon",
                "automatic",
                ControlValue::Boolean(false),
            )
            .unwrap();

        let controls = model.panels()[0].controls();

        assert_eq!(
            controls[0].value(),
            Some(ControlValueRef::Text("201.5 °C",)),
        );

        assert_eq!(controls[1].value(), Some(ControlValueRef::Number(200.0,)),);

        assert_eq!(controls[2].value(), Some(ControlValueRef::Boolean(false,)),);
    }

    #[test]
    fn rejects_wrong_value_type() {
        let mut model = model();

        let error = model
            .set_control_value(
                "metakon_script",
                "metakon",
                "setpoint",
                ControlValue::Text("two hundred".to_owned()),
            )
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Cannot assign text to number \
             control 'setpoint'",
        );
    }

    #[test]
    fn rejects_number_outside_range() {
        let mut model = model();

        let error = model
            .set_control_value(
                "metakon_script",
                "metakon",
                "setpoint",
                ControlValue::Number(1_500.0),
            )
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Number control 'setpoint' value \
             1500 exceeds maximum 1000",
        );
    }

    #[test]
    fn rejects_unknown_script_panel() {
        let mut model = model();

        let error = model
            .set_control_value(
                "unknown_script",
                "metakon",
                "setpoint",
                ControlValue::Number(200.0),
            )
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Control panel 'metakon' of \
             application script \
             'unknown_script' was not found",
        );
    }

    #[test]
    fn rejects_unknown_control() {
        let mut model = model();

        let error = model
            .set_control_value(
                "metakon_script",
                "metakon",
                "unknown",
                ControlValue::Number(200.0),
            )
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Control 'unknown' was not found \
             in panel 'metakon'",
        );
    }

    #[test]
    fn replaces_panels_of_same_script() {
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

        let mut model = ControlPanelModel::new();

        model.register_script("installation", &[first]);

        model.register_script("installation", &[second]);

        assert_eq!(model.panels().len(), 1,);

        assert_eq!(model.panels()[0].script_id(), "installation",);

        assert_eq!(model.panels()[0].id(), "vacuum",);

        assert_eq!(model.panels()[0].title(), "Vacuum system",);

        assert_eq!(model.panels()[0].controls().len(), 1,);
    }

    #[test]
    fn allows_same_panel_id_for_different_scripts() {
        let first = definition();
        let second = definition();

        let mut model = ControlPanelModel::new();

        model.register_script("first_script", &[first]);

        model.register_script("second_script", &[second]);

        assert_eq!(model.panels().len(), 2,);

        assert_eq!(model.panels()[0].script_id(), "first_script",);

        assert_eq!(model.panels()[1].script_id(), "second_script",);

        assert_eq!(model.panels()[0].id(), "metakon",);

        assert_eq!(model.panels()[1].id(), "metakon",);
    }

    #[test]
    fn unregisters_only_selected_script() {
        let first = definition();
        let second = definition();

        let mut model = ControlPanelModel::new();

        model.register_script("first_script", &[first]);

        model.register_script("second_script", &[second]);

        model.unregister_script("first_script");

        assert_eq!(model.panels().len(), 1,);

        assert_eq!(model.panels()[0].script_id(), "second_script",);
    }

    #[test]
    fn clears_all_panels() {
        let definition = definition();

        let mut model = ControlPanelModel::new();

        model.register_script("metakon_script", &[definition]);

        model.clear();

        assert!(model.panels().is_empty(),);
    }
}
