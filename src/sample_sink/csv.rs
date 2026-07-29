use std::{
    fs::{self, File},
    io::{self, BufWriter, Write},
    path::Path,
};

use crate::data::{SeriesMetadata, SeriesSample};

use super::{SampleSink, SampleSinkError};

pub struct CsvSampleSink<W> {
    writer: W,
}

impl CsvSampleSink<BufWriter<File>> {
    pub fn create(path: impl AsRef<Path>) -> Result<Self, SampleSinkError> {
        let path = path.as_ref();

        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }

        let file = File::create(path)?;

        Self::new(BufWriter::new(file))
    }
}

impl<W: Write> CsvSampleSink<W> {
    pub fn new(mut writer: W) -> Result<Self, SampleSinkError> {
        writeln!(writer, "timestamp,series_id,series_name,value",)?;

        writer.flush()?;

        Ok(Self { writer })
    }

    #[cfg(test)]
    pub fn into_inner(self) -> W {
        self.writer
    }
}

impl<W: Write + Send> SampleSink for CsvSampleSink<W> {
    fn write_batch(
        &mut self,
        samples: &[SeriesSample],
        series: &[SeriesMetadata],
    ) -> Result<(), SampleSinkError> {
        for series_sample in samples {
            let Some(metadata) = series
                .iter()
                .find(|metadata| metadata.id == series_sample.series_id)
            else {
                return Err(format!(
                    "Cannot write sample for unknown \
                     series {}",
                    series_sample.series_id,
                )
                .into());
            };

            write!(
                self.writer,
                "{},{},",
                series_sample.sample.timestamp, series_sample.series_id,
            )?;

            write_csv_field(&mut self.writer, &metadata.name)?;

            writeln!(self.writer, ",{}", series_sample.sample.value,)?;
        }

        self.writer.flush()?;

        Ok(())
    }

    fn flush(&mut self) -> Result<(), SampleSinkError> {
        self.writer.flush()?;

        Ok(())
    }
}

fn write_csv_field(writer: &mut impl Write, value: &str) -> io::Result<()> {
    let requires_quotes = value.contains([',', '"', '\r', '\n']);

    if requires_quotes {
        let escaped = value.replace('"', "\"\"");

        write!(writer, "\"{escaped}\"")
    } else {
        writer.write_all(value.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::CsvSampleSink;

    use crate::{
        data::{Sample, SeriesId, SeriesMetadata, SeriesSample, SeriesSource},
        sample_sink::SampleSink,
    };

    #[test]
    fn writes_header_names_and_samples() {
        let mut sink = CsvSampleSink::new(Vec::new()).unwrap();

        let series = [
            metadata(SeriesId::new(7), "temperature"),
            metadata(SeriesId::new(8), "light,sensor"),
        ];

        sink.write_batch(
            &[
                SeriesSample {
                    series_id: SeriesId::new(7),
                    sample: Sample::new(12.5, -3.25),
                },
                SeriesSample {
                    series_id: SeriesId::new(8),
                    sample: Sample::new(13.0, 4.5),
                },
            ],
            &series,
        )
        .unwrap();

        let output = String::from_utf8(sink.into_inner()).unwrap();

        assert_eq!(
            output,
            concat!(
                "timestamp,series_id,",
                "series_name,value\n",
                "12.5,7,temperature,-3.25\n",
                "13,8,\"light,sensor\",4.5\n",
            ),
        );
    }

    #[test]
    fn rejects_sample_without_metadata() {
        let mut sink = CsvSampleSink::new(Vec::new()).unwrap();

        let result = sink.write_batch(
            &[SeriesSample {
                series_id: SeriesId::new(7),
                sample: Sample::new(12.5, -3.25),
            }],
            &[],
        );

        assert_eq!(
            result.unwrap_err().to_string(),
            "Cannot write sample for unknown series 7",
        );
    }

    fn metadata(id: SeriesId, name: &str) -> SeriesMetadata {
        SeriesMetadata {
            id,
            name: name.to_owned(),

            source: SeriesSource::SerialCommand {
                command: "test".to_owned(),
                step: 1.0,
            },

            visible: true,
        }
    }
}
