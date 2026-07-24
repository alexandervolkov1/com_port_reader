use std::{
    io::{Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use serialport::{ClearBuffer, DataBits, FlowControl, Parity, SerialPort, StopBits};

use crate::device_emulator::DeviceEmulator;

const READ_TIMEOUT: Duration = Duration::from_millis(100);
const MAX_COMMAND_LENGTH: usize = 256;

pub struct DeviceEmulatorHandle {
    stop_requested: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<(), DeviceEmulatorHandleError>>>,
}

impl DeviceEmulatorHandle {
    pub fn start(
        port_name: impl Into<String>,
        baud_rate: u32,
    ) -> Result<Self, DeviceEmulatorHandleError> {
        let port_name = port_name.into();

        if port_name.trim().is_empty() {
            return Err(DeviceEmulatorHandleError::from(
                "Emulator COM port cannot be empty",
            ));
        }

        if baud_rate == 0 {
            return Err(DeviceEmulatorHandleError::from(
                "Emulator baud rate must be greater than zero",
            ));
        }

        let port = serialport::new(&port_name, baud_rate)
            .data_bits(DataBits::Eight)
            .parity(Parity::None)
            .stop_bits(StopBits::One)
            .flow_control(FlowControl::None)
            .timeout(READ_TIMEOUT)
            .open()?;

        port.clear(ClearBuffer::All)?;

        let stop_requested = Arc::new(AtomicBool::new(false));

        let thread_stop_requested = Arc::clone(&stop_requested);

        let thread = thread::Builder::new()
            .name(format!("device-emulator-{port_name}"))
            .spawn(move || run_emulator(port, thread_stop_requested))?;

        Ok(Self {
            stop_requested,
            thread: Some(thread),
        })
    }

    pub fn stop(&mut self) -> Result<(), DeviceEmulatorHandleError> {
        self.stop_requested.store(true, Ordering::Release);

        let Some(thread) = self.thread.take() else {
            return Ok(());
        };

        match thread.join() {
            Ok(result) => result,

            Err(_) => Err(DeviceEmulatorHandleError::from(
                "Device emulator thread panicked",
            )),
        }
    }
}

impl Drop for DeviceEmulatorHandle {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn run_emulator(
    mut port: Box<dyn SerialPort>,
    stop_requested: Arc<AtomicBool>,
) -> Result<(), DeviceEmulatorHandleError> {
    let mut emulator = DeviceEmulator::new();

    let mut command_buffer = Vec::new();
    let mut read_buffer = [0_u8; 64];

    while !stop_requested.load(Ordering::Acquire) {
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

            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}

            Err(error) => {
                return Err(error.into());
            }
        }
    }

    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceEmulatorHandleError {
    message: String,
}

impl std::fmt::Display for DeviceEmulatorHandleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for DeviceEmulatorHandleError {}

impl From<serialport::Error> for DeviceEmulatorHandleError {
    fn from(error: serialport::Error) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

impl From<std::io::Error> for DeviceEmulatorHandleError {
    fn from(error: std::io::Error) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

impl From<String> for DeviceEmulatorHandleError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<&str> for DeviceEmulatorHandleError {
    fn from(message: &str) -> Self {
        Self {
            message: message.to_owned(),
        }
    }
}
