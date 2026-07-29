use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serialport::{ClearBuffer, DataBits, FlowControl, Parity, SerialPort, StopBits};

use crate::{
    device_model::{DeviceModel, DeviceModelError},
    lua_device_model::LuaDeviceModel,
};

const READ_TIMEOUT: Duration = Duration::from_millis(100);

const MAX_COMMAND_LENGTH: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceEmulatorPortConfig {
    pub port_name: String,
    pub baud_rate: u32,
    pub data_bits: DataBits,
    pub parity: Parity,
    pub stop_bits: StopBits,
    pub flow_control: FlowControl,
}

pub struct DeviceEmulatorHandle {
    stop_requested: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<(), DeviceEmulatorHandleError>>>,
}

impl DeviceEmulatorHandle {
    pub fn start(
        config: DeviceEmulatorPortConfig,
        script_path: PathBuf,
    ) -> Result<Self, DeviceEmulatorHandleError> {
        if config.port_name.trim().is_empty() {
            return Err(DeviceEmulatorHandleError::from(
                "Emulator COM port cannot \
                     be empty",
            ));
        }

        if config.baud_rate == 0 {
            return Err(DeviceEmulatorHandleError::from(
                "Emulator baud rate must be \
                     greater than zero",
            ));
        }

        let port = serialport::new(&config.port_name, config.baud_rate)
            .data_bits(config.data_bits)
            .parity(config.parity)
            .stop_bits(config.stop_bits)
            .flow_control(config.flow_control)
            .timeout(READ_TIMEOUT)
            .open()?;

        port.clear(ClearBuffer::All)?;

        let stop_requested = Arc::new(AtomicBool::new(false));

        let thread_stop_requested = Arc::clone(&stop_requested);

        let thread_name = format!("device-emulator-{}", config.port_name,);

        let (startup_sender, startup_receiver) = mpsc::sync_channel(1);

        let thread = thread::Builder::new().name(thread_name).spawn(move || {
            run_emulator(port, thread_stop_requested, script_path, startup_sender)
        })?;

        match startup_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                stop_requested,
                thread: Some(thread),
            }),

            Ok(Err(error)) => {
                let _ = thread.join();

                Err(error)
            }

            Err(_) => {
                let error = match thread.join() {
                    Ok(Err(error)) => error,

                    Ok(Ok(())) => DeviceEmulatorHandleError::from(
                        "Device emulator stopped \
                             during startup",
                    ),

                    Err(_) => DeviceEmulatorHandleError::from(
                        "Device emulator thread \
                             panicked during startup",
                    ),
                };

                Err(error)
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
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
    script_path: PathBuf,
    startup_sender: SyncSender<Result<(), DeviceEmulatorHandleError>>,
) -> Result<(), DeviceEmulatorHandleError> {
    let mut model = match create_device_model(script_path) {
        Ok(model) => model,

        Err(error) => {
            let _ = startup_sender.send(Err(error.clone()));

            return Err(error);
        }
    };

    let started_at = Instant::now();

    if startup_sender.send(Ok(())).is_err() {
        return Ok(());
    }

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

                            let response = model.handle_command(&command, started_at.elapsed())?;

                            writeln!(port, "{response}",)?;

                            port.flush()?;
                        }

                        b'\r' => {}

                        value => {
                            if command_buffer.len() >= MAX_COMMAND_LENGTH {
                                command_buffer.clear();

                                writeln!(
                                    port,
                                    "error command is \
                                     too long",
                                )?;

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

fn create_device_model(path: PathBuf) -> Result<Box<dyn DeviceModel>, DeviceEmulatorHandleError> {
    let script = fs::read_to_string(&path).map_err(|error| {
        DeviceEmulatorHandleError::from(format!(
            "Failed to read Lua device script \
                 '{}': {error}",
            path.display(),
        ))
    })?;

    let model = LuaDeviceModel::from_source(&script).map_err(|error| {
        DeviceEmulatorHandleError::from(format!(
            "Failed to initialize Lua device \
                     script '{}': {error}",
            path.display(),
        ))
    })?;

    Ok(Box::new(model))
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

impl From<DeviceModelError> for DeviceEmulatorHandleError {
    fn from(error: DeviceModelError) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

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
