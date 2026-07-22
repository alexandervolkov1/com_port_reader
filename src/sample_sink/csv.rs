use std::io::Write;

use crate::data::SeriesSample;

use super::{SampleSink, SampleSinkError};

pub struct CsvSampleSink<W> {
    writer: W,
}

impl<W: Write> CsvSampleSink<W> {
    pub fn new(mut writer: W) -> Result<Self, SampleSinkError> {
        writeln!(writer, "timestamp,series_id,value")?;

        Ok(Self { writer })
    }

    pub fn into_inner(self) -> W {
        self.writer
    }
}

impl<W: Write + Send> SampleSink for CsvSampleSink<W> {
    fn write_batch(&mut self, samples: &[SeriesSample]) -> Result<(), SampleSinkError> {
        for series_sample in samples {
            writeln!(
                self.writer,
                "{},{},{}",
                series_sample.sample.timestamp, series_sample.series_id, series_sample.sample.value,
            )?;
        }

        Ok(())
    }

    fn flush(&mut self) -> Result<(), SampleSinkError> {
        self.writer.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::CsvSampleSink;

    use crate::{
        data::{Sample, SeriesId, SeriesSample},
        sample_sink::SampleSink,
    };

    #[test]
    fn writes_header_and_samples() {
        let mut sink = CsvSampleSink::new(Vec::new()).unwrap();

        sink.write_batch(&[
            SeriesSample {
                series_id: SeriesId::new(7),
                sample: Sample::new(12.5, -3.25),
            },
            SeriesSample {
                series_id: SeriesId::new(8),
                sample: Sample::new(13.0, 4.5),
            },
        ])
        .unwrap();

        sink.flush().unwrap();

        let output = String::from_utf8(sink.into_inner()).unwrap();

        assert_eq!(
            output,
            concat!(
                "timestamp,series_id,value\n",
                "12.5,7,-3.25\n",
                "13,8,4.5\n",
            ),
        );
    }
}
