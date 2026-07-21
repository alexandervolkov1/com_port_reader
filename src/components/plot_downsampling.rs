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

    let bucket_size = samples.len().div_ceil(target_buckets);

    output.reserve(target_buckets * 2 + 2);

    let mut last_added_index = None;

    push_sample(samples, 0, &mut last_added_index, output);

    for (bucket_number, bucket) in samples.chunks(bucket_size).enumerate() {
        let bucket_offset = bucket_number * bucket_size;

        let mut minimum_index = 0;
        let mut maximum_index = 0;

        for (index, sample) in bucket.iter().enumerate().skip(1) {
            if sample.value < bucket[minimum_index].value {
                minimum_index = index;
            }

            if sample.value > bucket[maximum_index].value {
                maximum_index = index;
            }
        }

        let minimum_index = bucket_offset + minimum_index;
        let maximum_index = bucket_offset + maximum_index;

        let (first_index, second_index) = if minimum_index <= maximum_index {
            (minimum_index, maximum_index)
        } else {
            (maximum_index, minimum_index)
        };

        push_sample(samples, first_index, &mut last_added_index, output);

        push_sample(samples, second_index, &mut last_added_index, output);
    }

    push_sample(samples, samples.len() - 1, &mut last_added_index, output);
}

fn push_sample(
    samples: &[Sample],
    index: usize,
    last_added_index: &mut Option<usize>,
    output: &mut Vec<PlotPoint>,
) {
    if *last_added_index == Some(index) {
        return;
    }

    output.push(plot_point_from_sample(samples[index]));

    *last_added_index = Some(index);
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
    fn preserves_endpoints_and_bucket_extrema() {
        let samples = [
            Sample::new(0.0, 0.0),
            Sample::new(1.0, 10.0),
            Sample::new(2.0, -10.0),
            Sample::new(3.0, 1.0),
            Sample::new(4.0, 2.0),
            Sample::new(5.0, 9.0),
            Sample::new(6.0, -9.0),
            Sample::new(7.0, 3.0),
        ];

        let mut points = Vec::new();

        downsample_min_max_into(&samples, 2, &mut points);

        assert_eq!(
            point_pairs(&points),
            vec![
                (0.0, 0.0),
                (1.0, 10.0),
                (2.0, -10.0),
                (5.0, 9.0),
                (6.0, -9.0),
                (7.0, 3.0),
            ],
        );
    }

    #[test]
    fn does_not_duplicate_points_for_constant_signal() {
        let samples = [
            Sample::new(0.0, 5.0),
            Sample::new(1.0, 5.0),
            Sample::new(2.0, 5.0),
            Sample::new(3.0, 5.0),
            Sample::new(4.0, 5.0),
            Sample::new(5.0, 5.0),
        ];

        let mut points = Vec::new();

        downsample_min_max_into(&samples, 2, &mut points);

        assert_eq!(
            point_pairs(&points),
            vec![(0.0, 5.0), (3.0, 5.0), (5.0, 5.0)],
        );
    }

    fn point_pairs(points: &[egui_plot::PlotPoint]) -> Vec<(f64, f64)> {
        points.iter().map(|point| (point.x, point.y)).collect()
    }
}
