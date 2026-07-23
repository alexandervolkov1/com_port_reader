use std::{
    env,
    error::Error,
    f64::consts::TAU,
    io::{self, Read, Write},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serialport::{ClearBuffer, DataBits, FlowControl, Parity, StopBits};

const DEFAULT_BAUD_RATE: u32 = 9_600;
const MAX_COMMAND_LENGTH: usize = 256;

struct DeviceEmulator {
    started_at: Instant,
    sine_amplitude: f64,
    sine_period: f64,
    noise_amplitude: f64,
    random_state: u64,
}

impl DeviceEmulator {
    fn new() -> Self {
        let random_state = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(1, |duration| duration.as_nanos() as u64 | 1);

        Self {
            started_at: Instant::now(),
            sine_amplitude: 1.0,
            sine_period: 30.0,
            noise_amplitude: 0.1,
            random_state,
        }
    }

    fn handle_command(&mut self, command: &str) -> String {
        let arguments = command.split_whitespace().collect::<Vec<_>>();

        match arguments.as_slice() {
            ["sin"] => self.sine_value().to_string(),

            ["noise"] => self.noise_value().to_string(),

            ["set", "sin_amplitude", value] => {
                set_non_negative(&mut self.sine_amplitude, value, "sin_amplitude")
            }

            ["set", "noise_amplitude", value] => {
                set_non_negative(&mut self.noise_amplitude, value, "noise_amplitude")
            }

            ["set", "sin_period", value] => {
                set_positive(&mut self.sine_period, value, "sin_period")
            }

            [] => "error empty command".to_owned(),

            _ => format!("error unknown command: {command}",),
        }
    }

    fn sine_value(&self) -> f64 {
        let elapsed = self.started_at.elapsed().as_secs_f64();

        let phase = TAU * elapsed / self.sine_period;

        self.sine_amplitude * phase.sin()
    }

    fn noise_value(&mut self) -> f64 {
        // Простой xorshift64 без дополнительной зависимости rand.
        self.random_state ^= self.random_state << 13;
        self.random_state ^= self.random_state >> 7;
        self.random_state ^= self.random_state << 17;

        let normalized = self.random_state as f64 / u64::MAX as f64;

        let bipolar = normalized * 2.0 - 1.0;

        self.noise_amplitude * bipolar
    }
}

fn set_non_negative(target: &mut f64, value: &str, parameter_name: &str) -> String {
    let Ok(value) = value.parse::<f64>() else {
        return format!("error invalid value for {parameter_name}",);
    };

    if !value.is_finite() || value < 0.0 {
        return format!(
            "error {parameter_name} must be \
             finite and non-negative",
        );
    }

    *target = value;

    format!("ok {parameter_name} {value}")
}

fn set_positive(target: &mut f64, value: &str, parameter_name: &str) -> String {
    let Ok(value) = value.parse::<f64>() else {
        return format!("error invalid value for {parameter_name}",);
    };

    if !value.is_finite() || value <= 0.0 {
        return format!(
            "error {parameter_name} must be \
             finite and greater than zero",
        );
    }

    *target = value;

    format!("ok {parameter_name} {value}")
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Device emulator failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args().skip(1);

    let port_name = arguments.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing COM port; usage: \
             device_emulator <PORT> [BAUD]",
        )
    })?;

    let baud_rate = match arguments.next() {
        Some(value) => value.parse::<u32>().map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "invalid baud rate '{value}': \
                         {error}",
                ),
            )
        })?,

        None => DEFAULT_BAUD_RATE,
    };

    if arguments.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "too many arguments; usage: \
                 device_emulator <PORT> [BAUD]",
        )
        .into());
    }

    let mut port = serialport::new(&port_name, baud_rate)
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::One)
        .flow_control(FlowControl::None)
        .timeout(Duration::from_millis(100))
        .open()?;

    port.clear(ClearBuffer::All)?;

    println!(
        "Device emulator is running on \
         {port_name} at {baud_rate} baud.",
    );

    println!("Commands:");
    println!("  sin");
    println!("  noise");
    println!("  set sin_amplitude <value>");
    println!("  set noise_amplitude <value>");
    println!("  set sin_period <seconds>");
    println!("Press Ctrl+C to stop.");

    let mut emulator = DeviceEmulator::new();
    let mut command_buffer = Vec::new();
    let mut read_buffer = [0_u8; 64];

    loop {
        match port.read(&mut read_buffer) {
            Ok(0) => {}

            Ok(bytes_read) => {
                for &byte in &read_buffer[..bytes_read] {
                    match byte {
                        b'\n' => {
                            let command =
                                String::from_utf8_lossy(&command_buffer).trim().to_owned();

                            command_buffer.clear();

                            let response = emulator.handle_command(&command);

                            println!("< {command}");
                            println!("> {response}");

                            writeln!(port, "{response}")?;
                            port.flush()?;
                        }

                        b'\r' => {}

                        value => {
                            if command_buffer.len() >= MAX_COMMAND_LENGTH {
                                command_buffer.clear();

                                writeln!(port, "error command is too long",)?;

                                port.flush()?;
                            } else {
                                command_buffer.push(value);
                            }
                        }
                    }
                }
            }

            Err(error) if error.kind() == io::ErrorKind::TimedOut => {}

            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DeviceEmulator;

    #[test]
    fn changes_sine_amplitude() {
        let mut emulator = DeviceEmulator::new();

        assert_eq!(
            emulator.handle_command("set sin_amplitude 2.5",),
            "ok sin_amplitude 2.5",
        );

        assert_eq!(emulator.sine_amplitude, 2.5);
    }

    #[test]
    fn changes_noise_amplitude() {
        let mut emulator = DeviceEmulator::new();

        assert_eq!(
            emulator.handle_command("set noise_amplitude 0.25",),
            "ok noise_amplitude 0.25",
        );

        assert_eq!(emulator.noise_amplitude, 0.25);
    }

    #[test]
    fn rejects_invalid_period() {
        let mut emulator = DeviceEmulator::new();

        let response = emulator.handle_command("set sin_period 0");

        assert!(response.starts_with("error"));
    }

    #[test]
    fn returns_numeric_values() {
        let mut emulator = DeviceEmulator::new();

        assert!(emulator.handle_command("sin").parse::<f64>().is_ok(),);

        assert!(emulator.handle_command("noise").parse::<f64>().is_ok(),);
    }
}
