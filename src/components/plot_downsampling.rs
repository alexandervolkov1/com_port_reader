use egui_plot::PlotPoint;

use crate::data::Sample;

pub(super) fn downsample_min_max_into(
    samples: &[Sample],
    target_buckets: usize,
    output: &mut Vec<PlotPoint>,
) {
    if samples.len() <= target_buckets || target_buckets < 2 {
        output.extend(samples.iter().copied().map(plot_point_from_sample));

        return;
    }

    let bucket_size = samples.len() as f64 / target_buckets as f64;

    output.reserve(target_buckets * 2);

    let mut bucket_start = 0.0;

    while (bucket_start as usize) < samples.len() {
        let start = bucket_start as usize;
        let end = ((bucket_start + bucket_size) as usize).min(samples.len());

        if start >= end {
            break;
        }

        let bucket = &samples[start..end];

        let mut minimum = bucket[0];
        let mut maximum = bucket[0];

        for sample in bucket {
            if sample.value < minimum.value {
                minimum = *sample;
            }

            if sample.value > maximum.value {
                maximum = *sample;
            }
        }

        if minimum.timestamp < maximum.timestamp {
            output.push(plot_point_from_sample(minimum));
            output.push(plot_point_from_sample(maximum));
        } else {
            output.push(plot_point_from_sample(maximum));
            output.push(plot_point_from_sample(minimum));
        }

        bucket_start += bucket_size;
    }
}

fn plot_point_from_sample(sample: Sample) -> PlotPoint {
    PlotPoint {
        x: sample.timestamp,
        y: sample.value,
    }
}

#[cfg(test)]
mod tests {
    use super::downsample_min_max_into;

    use crate::data::Sample;

    #[test]
    fn preserves_samples_when_downsampling_is_not_needed() {
        let samples = [
            Sample::new(1.0, 10.0),
            Sample::new(2.0, 20.0),
            Sample::new(3.0, 30.0),
        ];

        let mut points = Vec::new();

        downsample_min_max_into(&samples, 10, &mut points);

        assert_eq!(
            point_pairs(&points),
            vec![(1.0, 10.0), (2.0, 20.0), (3.0, 30.0)],
        );
    }

    #[test]
    fn preserves_minimum_and_maximum_in_chronological_order() {
        let samples = [
            Sample::new(0.0, 1.0),
            Sample::new(1.0, 5.0),
            Sample::new(2.0, -2.0),
            Sample::new(3.0, 7.0),
            Sample::new(4.0, 0.0),
            Sample::new(5.0, 3.0),
        ];

        let mut points = Vec::new();

        downsample_min_max_into(&samples, 2, &mut points);

        assert_eq!(
            point_pairs(&points),
            vec![(1.0, 5.0), (2.0, -2.0), (3.0, 7.0), (4.0, 0.0)],
        );
    }

    fn point_pairs(points: &[egui_plot::PlotPoint]) -> Vec<(f64, f64)> {
        points.iter().map(|point| (point.x, point.y)).collect()
    }
}
