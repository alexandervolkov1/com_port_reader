mod signal_generator;

use crate::data::SignalSeries;

pub use signal_generator::SignalGenerator;

pub trait AcquisitionSource: Send {
    fn sample(&mut self, series: &mut [SignalSeries], timestamp: f64, elapsed_seconds: f64);
}
