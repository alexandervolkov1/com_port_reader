pub mod sample;
pub mod sampling_interval;
pub mod series;
pub mod series_name;
pub mod series_sample;
pub mod series_store;

pub use sample::Sample;
pub use sampling_interval::{SamplingInterval, SamplingIntervalError};
pub use series::{
    DEFAULT_METAKON_CHANNEL, DEFAULT_METAKON_DEVICE, DEFAULT_METAKON_SCALE, NewSeries, Series,
    SeriesId, SeriesMetadata, SeriesSource,
};
pub use series_name::SeriesNameError;
pub use series_sample::SeriesSample;
pub use series_store::{AddSeriesError, RenameSeriesError, SeriesStore};
