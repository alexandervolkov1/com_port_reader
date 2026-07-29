pub mod sample;
pub mod series;
pub mod series_name;
pub mod series_sample;
pub mod series_store;

pub use sample::Sample;
pub use series::{
    DEFAULT_METAKON_CHANNEL, DEFAULT_METAKON_DEVICE, DEFAULT_METAKON_REGISTER,
    DEFAULT_METAKON_SCALE, DEFAULT_SERIAL_STEP, NewSeries, SeriesId, SeriesMetadata, SeriesSource,
    SignalSeries,
};
pub use series_name::SeriesNameError;
pub use series_sample::SeriesSample;
pub use series_store::{AddSeriesError, RenameSeriesError, SeriesStore};
