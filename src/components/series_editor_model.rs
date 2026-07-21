use crate::data::{NewSeries, Signal, SignalKind, SignalValidationError};

pub struct SeriesDraft {
    pub name: String,
    pub kind: SignalKind,

    pub amplitude: f64,
    pub period: f64,
    pub phase: f64,
    pub duty_cycle: f64,
    pub value: f64,
}

impl SeriesDraft {
    fn build(&self) -> Result<NewSeries, SignalValidationError> {
        let signal = match self.kind {
            SignalKind::Sine => Signal::SineWave {
                amplitude: self.amplitude,
                period: self.period,
                phase: self.phase,
            },

            SignalKind::Square => Signal::SquareWave {
                amplitude: self.amplitude,
                period: self.period,
                duty_cycle: self.duty_cycle,
            },

            SignalKind::Triangle => Signal::TriangleWave {
                amplitude: self.amplitude,
                period: self.period,
            },

            SignalKind::Sawtooth => Signal::SawtoothWave {
                amplitude: self.amplitude,
                period: self.period,
            },

            SignalKind::Constant => Signal::Constant { value: self.value },
        };

        signal.validate()?;

        let name = self.name.trim();

        Ok(if name.is_empty() {
            NewSeries::unnamed(signal)
        } else {
            NewSeries::named(signal, name)
        })
    }
}

impl Default for SeriesDraft {
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: SignalKind::Sine,
            amplitude: 100.0,
            period: 100.0,
            phase: 0.0,
            duty_cycle: 0.5,
            value: 50.0,
        }
    }
}

#[derive(Default)]
pub struct SeriesEditorModel {
    open: bool,
    draft: SeriesDraft,
    error: Option<String>,
}

impl SeriesEditorModel {
    pub fn open(&mut self) {
        *self = Self {
            open: true,
            ..Self::default()
        };
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub const fn is_open(&self) -> bool {
        self.open
    }

    pub fn draft_mut(&mut self) -> &mut SeriesDraft {
        &mut self.draft
    }

    pub fn build(&self) -> Result<NewSeries, SignalValidationError> {
        self.draft.build()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }
}

#[cfg(test)]
mod tests {
    use super::SeriesDraft;

    use crate::data::{Signal, SignalKind, SignalValidationError};

    #[test]
    fn builds_default_sine_series() {
        let new_series = SeriesDraft::default().build().unwrap();

        let (signal, name) = new_series.into_parts();

        assert_eq!(name, None);
        assert_eq!(
            signal,
            Signal::SineWave {
                amplitude: 100.0,
                period: 100.0,
                phase: 0.0,
            },
        );
    }

    #[test]
    fn builds_named_square_series() {
        let draft = SeriesDraft {
            name: "  pulse  ".to_owned(),
            kind: SignalKind::Square,
            amplitude: 10.0,
            period: 20.0,
            duty_cycle: 0.25,
            ..SeriesDraft::default()
        };

        let (signal, name) = draft.build().unwrap().into_parts();

        assert_eq!(name.as_deref(), Some("pulse"));
        assert_eq!(
            signal,
            Signal::SquareWave {
                amplitude: 10.0,
                period: 20.0,
                duty_cycle: 0.25,
            },
        );
    }

    #[test]
    fn rejects_invalid_parameters() {
        let draft = SeriesDraft {
            period: 0.0,
            ..SeriesDraft::default()
        };

        assert_eq!(
            draft.build().unwrap_err(),
            SignalValidationError::NotPositive("Period"),
        );
    }
}
