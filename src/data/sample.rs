#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub timestamp: f64,
    pub value: f64,
}

impl Sample {
    pub const fn new(timestamp: f64, value: f64) -> Self {
        Self { timestamp, value }
    }
}
