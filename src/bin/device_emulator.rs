use std::{
    env,
    error::Error,
    io::{self, Read, Write},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serialport::{ClearBuffer, DataBits, FlowControl, Parity, StopBits};

const DEFAULT_BAUD_RATE: u32 = 9_600;
const MAX_COMMAND_LENGTH: usize = 256;

struct DeviceEmulator {
    accumulated_value: i64,
    random_state: u64,
}

impl DeviceEmulator {
    fn new() -> Self {
        let random_state = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(1, |duration| duration.as_nanos() as u64 | 1);

        Self {
            accumulated_value: 0,
            random_state,
        }
    }

    fn handle_command(&mut self, command: &str) -> String {
        match command.trim() {
            "get" => {
                self.accumulated_value += self.random_step();

                format!("{:.1}", self.accumulated_value as f64,)
            }

            "" => "error empty command".to_owned(),

            command => format!("error unknown command: {command}",),
        }
    }

    fn random_step(&mut self) -> i64 {
        // Простой xorshift64.
        self.random_state ^= self.random_state << 13;
        self.random_state ^= self.random_state >> 7;
        self.random_state ^= self.random_state << 17;

        if self.random_state & 1 == 0 { -1 } else { 1 }
    }
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
        "Random walk emulator is running on \
         {port_name} at {baud_rate} baud.",
    );

    println!("Command: get");
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
    fn moves_one_step_for_each_request() {
        let mut emulator = DeviceEmulator::new();
        let mut previous_value = 0.0_f64;

        for _ in 0..100 {
            let current_value = emulator.handle_command("get").parse::<f64>().unwrap();

            assert_eq!((current_value - previous_value).abs(), 1.0,);

            previous_value = current_value;
        }
    }

    #[test]
    fn rejects_unknown_command() {
        let mut emulator = DeviceEmulator::new();

        let response = emulator.handle_command("unknown");

        assert!(response.starts_with("error"));
    }

    #[test]
    fn rejects_empty_command() {
        let mut emulator = DeviceEmulator::new();

        assert_eq!(emulator.handle_command("   "), "error empty command",);
    }
}
