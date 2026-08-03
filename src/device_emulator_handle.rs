use std::{
    fs,
    io::Read,
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
    lua_virtual_instrument_model::LuaVirtualInstrumentModel,
    protocol::virtual_instrument::{
        VirtualFrameDecoder, VirtualFrameError, VirtualFrameIoError, VirtualInstrumentMessage,
        VirtualInstrumentModelError, VirtualInstrumentServer, VirtualMessageCodecError,
        write_frame,
    },
};

const READ_TIMEOUT: Duration = Duration::from_millis(100);

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
    let model = match create_device_model(script_path) {
        Ok(model) => model,

        Err(error) => {
            let _ = startup_sender.send(Err(error.clone()));

            return Err(error);
        }
    };

    let mut server = VirtualInstrumentServer::new(model);

    let started_at = Instant::now();

    if startup_sender.send(Ok(())).is_err() {
        return Ok(());
    }

    let mut decoder = VirtualFrameDecoder::new();

    let mut read_buffer = [0_u8; 256];

    while !stop_requested.load(Ordering::Acquire) {
        match port.read(&mut read_buffer) {
            Ok(0) => {}

            Ok(bytes_read) => {
                decoder.push(&read_buffer[..bytes_read]);

                loop {
                    let Some(frame) = decoder.next_frame()? else {
                        break;
                    };

                    let request = VirtualInstrumentMessage::decode_frame(&frame)?;

                    let response = server.handle(request, started_at.elapsed());

                    let response_frame = response.encode_frame()?;

                    write_frame(port.as_mut(), &response_frame)?;
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

fn create_device_model(
    path: PathBuf,
) -> Result<LuaVirtualInstrumentModel, DeviceEmulatorHandleError> {
    let script = fs::read_to_string(&path).map_err(|error| {
        DeviceEmulatorHandleError::from(format!(
            "Failed to read Lua device \
                         script '{}': {error}",
            path.display(),
        ))
    })?;

    LuaVirtualInstrumentModel::from_source(&script).map_err(|error| {
        DeviceEmulatorHandleError::from(format!(
            "Failed to initialize Lua virtual \
                 instrument script '{}': {error}",
            path.display(),
        ))
    })
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

impl From<VirtualInstrumentModelError> for DeviceEmulatorHandleError {
    fn from(error: VirtualInstrumentModelError) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

impl From<VirtualFrameError> for DeviceEmulatorHandleError {
    fn from(error: VirtualFrameError) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

impl From<VirtualFrameIoError> for DeviceEmulatorHandleError {
    fn from(error: VirtualFrameIoError) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

impl From<VirtualMessageCodecError> for DeviceEmulatorHandleError {
    fn from(error: VirtualMessageCodecError) -> Self {
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
