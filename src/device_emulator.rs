use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

const DEFAULT_WALK_ID: &str = "default";
const DEFAULT_STEP: f64 = 1.0;

pub struct DeviceEmulator {
    walks: HashMap<String, RandomWalk>,
    base_seed: u64,
}

impl DeviceEmulator {
    pub fn new() -> Self {
        let base_seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(1, |duration| duration.as_nanos() as u64 | 1);

        Self {
            walks: HashMap::new(),
            base_seed,
        }
    }

    pub fn handle_command(&mut self, command: &str) -> String {
        let command = command.trim();

        if command.is_empty() {
            return "error empty command".to_owned();
        }

        let mut tokens = command.split_whitespace();

        match tokens.next() {
            Some("get") => self.handle_get(tokens),

            Some(_) => {
                format!("error unknown command: {command}")
            }

            None => "error empty command".to_owned(),
        }
    }

    fn handle_get<'a>(&mut self, mut tokens: impl Iterator<Item = &'a str>) -> String {
        let walk_id = tokens.next().unwrap_or(DEFAULT_WALK_ID);

        let step = match tokens.next() {
            Some(value) => match parse_step(value) {
                Ok(step) => step,
                Err(error) => return error,
            },

            None => DEFAULT_STEP,
        };

        if let Some(argument) = tokens.next() {
            return format!("error unexpected argument: {argument}");
        }

        let seed = seed_for_walk(self.base_seed, walk_id);

        let walk = self
            .walks
            .entry(walk_id.to_owned())
            .or_insert_with(|| RandomWalk::new(seed));

        walk.advance(step).to_string()
    }

    #[cfg(test)]
    fn with_seed(base_seed: u64) -> Self {
        Self {
            walks: HashMap::new(),
            base_seed: base_seed.max(1),
        }
    }
}

impl Default for DeviceEmulator {
    fn default() -> Self {
        Self::new()
    }
}

struct RandomWalk {
    value: f64,
    random_state: u64,
}

impl RandomWalk {
    fn new(random_state: u64) -> Self {
        Self {
            value: 0.0,
            random_state,
        }
    }

    fn advance(&mut self, step: f64) -> f64 {
        // Each walk owns its own xorshift64 state.
        self.random_state ^= self.random_state << 13;
        self.random_state ^= self.random_state >> 7;
        self.random_state ^= self.random_state << 17;

        if self.random_state & 1 == 0 {
            self.value -= step;
        } else {
            self.value += step;
        }

        self.value
    }
}

fn parse_step(value: &str) -> Result<f64, String> {
    let step = value
        .parse::<f64>()
        .map_err(|error| format!("error invalid step '{value}': {error}"))?;

    if !step.is_finite() || step <= 0.0 {
        return Err(format!(
            "error step must be finite and greater \
             than zero: {value}"
        ));
    }

    Ok(step)
}

fn seed_for_walk(base_seed: u64, walk_id: &str) -> u64 {
    let mut seed = base_seed ^ 0xcbf2_9ce4_8422_2325;

    for byte in walk_id.bytes() {
        seed ^= u64::from(byte);

        seed = seed.wrapping_mul(0x0000_0100_0000_01b3);
    }

    // xorshift64 must not start from zero.
    seed | 1
}

#[cfg(test)]
mod tests {
    use super::DeviceEmulator;

    fn response_value(emulator: &mut DeviceEmulator, command: &str) -> f64 {
        emulator.handle_command(command).parse::<f64>().unwrap()
    }

    #[test]
    fn keeps_plain_get_backward_compatible() {
        let mut emulator = DeviceEmulator::with_seed(1);

        let mut previous_value = 0.0;

        for _ in 0..100 {
            let current_value = response_value(&mut emulator, "get");

            assert_eq!((current_value - previous_value).abs(), 1.0,);

            previous_value = current_value;
        }
    }

    #[test]
    fn uses_configured_step() {
        let mut emulator = DeviceEmulator::with_seed(1);

        let mut previous_value = 0.0;

        for _ in 0..100 {
            let current_value = response_value(&mut emulator, "get slow 0.25");

            assert_eq!((current_value - previous_value).abs(), 0.25,);

            previous_value = current_value;
        }
    }

    #[test]
    fn keeps_walk_random_states_independent() {
        let mut interleaved = DeviceEmulator::with_seed(42);

        let first_before = response_value(&mut interleaved, "get first 2");

        response_value(&mut interleaved, "get second 0.5");

        let first_after = response_value(&mut interleaved, "get first 2");

        let mut isolated = DeviceEmulator::with_seed(42);

        let expected_before = response_value(&mut isolated, "get first 2");

        let expected_after = response_value(&mut isolated, "get first 2");

        assert_eq!(first_before, expected_before);
        assert_eq!(first_after, expected_after);
        assert_eq!(interleaved.walks.len(), 2);
    }

    #[test]
    fn rejects_invalid_step() {
        let mut emulator = DeviceEmulator::with_seed(1);

        assert!(emulator.handle_command("get walk 0").starts_with("error"));

        assert!(emulator.handle_command("get walk NaN").starts_with("error"));
    }

    #[test]
    fn rejects_extra_argument() {
        let mut emulator = DeviceEmulator::with_seed(1);

        assert_eq!(
            emulator.handle_command("get walk 1 extra"),
            "error unexpected argument: extra",
        );
    }

    #[test]
    fn rejects_unknown_command() {
        let mut emulator = DeviceEmulator::with_seed(1);

        assert_eq!(
            emulator.handle_command("unknown"),
            "error unknown command: unknown",
        );
    }

    #[test]
    fn rejects_empty_command() {
        let mut emulator = DeviceEmulator::with_seed(1);

        assert_eq!(emulator.handle_command("   "), "error empty command",);
    }
}
