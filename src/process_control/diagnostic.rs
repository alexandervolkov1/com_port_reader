
use std::fmt;

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
)]
pub enum ControllerDiagnostic {
    Setpoint,
    Proportional,
    Integral,
    Derivative,
    Output,
    UnconstrainedOutput,
}

impl ControllerDiagnostic {
    pub const ALL: [Self; 6] = [
        Self::Setpoint,
        Self::Proportional,
        Self::Integral,
        Self::Derivative,
        Self::Output,
        Self::UnconstrainedOutput,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Setpoint => "setpoint",
            Self::Proportional => "proportional",
            Self::Integral => "integral",
            Self::Derivative => "derivative",
            Self::Output => "output",
            Self::UnconstrainedOutput => {
                "unconstrained_output"
            }
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|diagnostic| {
                diagnostic.key() == key
            })
    }
}

impl fmt::Display for ControllerDiagnostic {
    fn fmt(
        &self,
        formatter: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        formatter.write_str(self.key())
    }
}

#[cfg(test)]
mod tests {
    use super::ControllerDiagnostic;

    #[test]
    fn finds_controller_diagnostic_by_key() {
        assert_eq!(
            ControllerDiagnostic::from_key(
                "setpoint",
            ),
            Some(
                ControllerDiagnostic::Setpoint,
            ),
        );

        assert_eq!(
            ControllerDiagnostic::from_key(
                "proportional",
            ),
            Some(
                ControllerDiagnostic::
                    Proportional,
            ),
        );

        assert_eq!(
            ControllerDiagnostic::from_key(
                "integral",
            ),
            Some(
                ControllerDiagnostic::Integral,
            ),
        );

        assert_eq!(
            ControllerDiagnostic::from_key(
                "derivative",
            ),
            Some(
                ControllerDiagnostic::
                    Derivative,
            ),
        );

        assert_eq!(
            ControllerDiagnostic::from_key(
                "output",
            ),
            Some(
                ControllerDiagnostic::Output,
            ),
        );

        assert_eq!(
            ControllerDiagnostic::from_key(
                "unconstrained_output",
            ),
            Some(
                ControllerDiagnostic::
                    UnconstrainedOutput,
            ),
        );

        assert_eq!(
            ControllerDiagnostic::from_key(
                "missing",
            ),
            None,
        );
    }
}
