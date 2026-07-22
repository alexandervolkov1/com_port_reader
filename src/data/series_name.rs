#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeriesNameError {
    Empty,
    ContainsWhitespace,
    Duplicate(String),
}

impl std::fmt::Display for SeriesNameError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("Series name cannot be empty"),

            Self::ContainsWhitespace => {
                formatter.write_str("Series name cannot contain whitespace")
            }

            Self::Duplicate(name) => {
                write!(formatter, "Series name '{name}' already exists",)
            }
        }
    }
}

impl std::error::Error for SeriesNameError {}

pub(crate) fn normalize_series_name(name: &str) -> Result<&str, SeriesNameError> {
    let name = name.trim();

    if name.is_empty() {
        return Err(SeriesNameError::Empty);
    }

    if name.chars().any(char::is_whitespace) {
        return Err(SeriesNameError::ContainsWhitespace);
    }

    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::{SeriesNameError, normalize_series_name};

    #[test]
    fn trims_valid_name() {
        assert_eq!(normalize_series_name("  temperature  "), Ok("temperature"),);
    }

    #[test]
    fn rejects_empty_name() {
        assert_eq!(normalize_series_name("   "), Err(SeriesNameError::Empty),);
    }

    #[test]
    fn rejects_whitespace_inside_name() {
        assert_eq!(
            normalize_series_name("room temperature"),
            Err(SeriesNameError::ContainsWhitespace),
        );
    }
}
