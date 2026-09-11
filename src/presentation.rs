use std::{collections::HashSet, error::Error, fmt};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlotPaneKey(String);

impl PlotPaneKey {
    pub fn new(value: impl Into<String>) -> Result<Self, PresentationDefinitionError> {
        let value = value.into();

        validate_identifier("plot pane", &value)?;

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PlotPaneKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlotPaneDefinition {
    key: PlotPaneKey,
    title: String,
    weight: f32,
}

impl PlotPaneDefinition {
    pub fn new(
        key: PlotPaneKey,
        title: impl Into<String>,
        weight: f32,
    ) -> Result<Self, PresentationDefinitionError> {
        let title = title.into();

        if title.trim().is_empty() {
            return Err(PresentationDefinitionError::new(
                "Plot pane title cannot be empty",
            ));
        }

        if !weight.is_finite() || weight <= 0.0 {
            return Err(PresentationDefinitionError::new(
                "Plot pane weight must be finite and greater than zero",
            ));
        }

        Ok(Self { key, title, weight })
    }

    pub const fn key(&self) -> &PlotPaneKey {
        &self.key
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub const fn weight(&self) -> f32 {
        self.weight
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlotLayoutDefinition {
    panes: Vec<PlotPaneDefinition>,
}

impl PlotLayoutDefinition {
    pub fn new(
        panes: impl IntoIterator<Item = PlotPaneDefinition>,
    ) -> Result<Self, PresentationDefinitionError> {
        let panes = panes.into_iter().collect::<Vec<_>>();

        if panes.is_empty() {
            return Err(PresentationDefinitionError::new(
                "Plot layout must contain at least one pane",
            ));
        }

        let mut keys = HashSet::new();

        for pane in &panes {
            if !keys.insert(pane.key().clone()) {
                return Err(PresentationDefinitionError::new(format!(
                    "Plot pane '{}' is defined more than once",
                    pane.key(),
                )));
            }
        }

        Ok(Self { panes })
    }

    pub fn panes(&self) -> &[PlotPaneDefinition] {
        &self.panes
    }

    pub fn contains(&self, key: &PlotPaneKey) -> bool {
        self.panes.iter().any(|pane| pane.key() == key)
    }
}

impl Default for PlotLayoutDefinition {
    fn default() -> Self {
        Self {
            panes: vec![PlotPaneDefinition {
                key: PlotPaneKey("main".to_owned()),
                title: "Plot".to_owned(),
                weight: 1.0,
            }],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationDefinitionError {
    message: String,
}

impl PresentationDefinitionError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for PresentationDefinitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for PresentationDefinitionError {}

fn validate_identifier(kind: &str, identifier: &str) -> Result<(), PresentationDefinitionError> {
    let mut characters = identifier.chars();

    let valid_first = characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_');
    let valid_remaining =
        characters.all(|character| character.is_ascii_alphanumeric() || character == '_');

    if !valid_first || !valid_remaining {
        return Err(PresentationDefinitionError::new(format!(
            "Invalid {kind} id '{identifier}': use an ASCII letter or underscore first, followed by letters, digits or underscores",
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PlotLayoutDefinition, PlotPaneDefinition, PlotPaneKey};

    #[test]
    fn validates_plot_layout() {
        let pane =
            PlotPaneDefinition::new(PlotPaneKey::new("temperature").unwrap(), "Temperature", 2.0)
                .unwrap();

        assert_eq!(
            PlotLayoutDefinition::new([pane.clone()]).unwrap().panes(),
            &[pane]
        );
        assert!(PlotLayoutDefinition::new(Vec::new()).is_err());
    }

    #[test]
    fn rejects_duplicate_keys() {
        let pane = PlotPaneDefinition::new(PlotPaneKey::new("main").unwrap(), "Main", 1.0).unwrap();

        assert!(PlotLayoutDefinition::new([pane.clone(), pane]).is_err());
    }
}
