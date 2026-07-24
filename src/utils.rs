use std::time::{SystemTime, UNIX_EPOCH};

const INVALID_TIME_LABEL: &str = "invalid time";

pub fn mark_for_timestamp(timestamp: f64) -> String {
    if !timestamp.is_finite() {
        return INVALID_TIME_LABEL.to_owned();
    }

    let Some(datetime) = chrono::DateTime::from_timestamp(timestamp as i64, 0) else {
        return INVALID_TIME_LABEL.to_owned();
    };

    datetime
        .with_timezone(&chrono::Local)
        .format("%H:%M:%S")
        .to_string()
}

pub fn current_time_f64() -> f64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs_f64(),

        Err(error) => -error.duration().as_secs_f64(),
    }
}

#[cfg(test)]
mod tests {
    use super::mark_for_timestamp;

    #[test]
    fn formats_valid_timestamp() {
        let formatted = mark_for_timestamp(1_000_000.0);

        assert_eq!(formatted.len(), 8);
    }

    #[test]
    fn handles_non_finite_timestamp() {
        assert_eq!(mark_for_timestamp(f64::NAN), "invalid time",);

        assert_eq!(mark_for_timestamp(f64::INFINITY), "invalid time",);
    }

    #[test]
    fn handles_out_of_range_timestamp() {
        assert_eq!(mark_for_timestamp(f64::MAX), "invalid time",);
    }
}
