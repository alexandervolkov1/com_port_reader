#[path = "../device_model.rs"]
mod device_model;

#[path = "../lua_device_model.rs"]
mod lua_device_model;

#[path = "../lua_execution.rs"]
mod lua_execution;

#[path = "../device_emulator.rs"]
mod device_emulator;

#[path = "../device_emulator_handle.rs"]
mod device_emulator_handle;

use std::{
    env,
    error::Error,
    io,
    path::PathBuf,
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::Duration,
};

use device_emulator_handle::{DeviceEmulatorHandle, DeviceEmulatorPortConfig};

use device_model::DeviceModelSource;

use serialport::{DataBits, FlowControl, Parity, StopBits};

const DEFAULT_BAUD_RATE: u32 = 9_600;
const STATUS_POLL_INTERVAL: Duration = Duration::from_millis(100);

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
             device_emulator <PORT> [BAUD] \
             [LUA_SCRIPT]",
        )
    })?;

    let baud_rate = match arguments.next() {
        Some(value) => value.parse::<u32>().map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "invalid baud rate \
                         '{value}': {error}",
                ),
            )
        })?,

        None => DEFAULT_BAUD_RATE,
    };

    let script_path = arguments.next().map(PathBuf::from);

    if arguments.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "too many arguments; usage: \
             device_emulator <PORT> [BAUD] \
             [LUA_SCRIPT]",
        )
        .into());
    }

    let config = DeviceEmulatorPortConfig {
        port_name: port_name.clone(),
        baud_rate,
        data_bits: DataBits::Eight,
        parity: Parity::None,
        stop_bits: StopBits::One,
        flow_control: FlowControl::None,
    };

    let model_source = match script_path {
        Some(path) => DeviceModelSource::LuaScript(path),

        None => DeviceModelSource::BuiltIn,
    };

    let built_in = matches!(&model_source, DeviceModelSource::BuiltIn,);

    let model_description = match &model_source {
        DeviceModelSource::BuiltIn => "built-in random walk".to_owned(),

        DeviceModelSource::LuaScript(path) => {
            format!("Lua script '{}'", path.display(),)
        }
    };

    let mut emulator = DeviceEmulatorHandle::start(config, model_source)?;

    println!(
        "Device emulator ({model_description}) \
         is running on {port_name} at \
         {baud_rate} baud.",
    );

    if built_in {
        println!("Commands:");
        println!("  walk");
        println!("  walk <walk-id> [step]");
    }

    println!("Press Enter to stop.");

    let (stop_sender, stop_receiver) = mpsc::channel();

    let _input_thread = thread::spawn(move || {
        let mut input = String::new();

        let _read_result = io::stdin().read_line(&mut input);

        let _send_result = stop_sender.send(());
    });

    while emulator.is_running() {
        match stop_receiver.recv_timeout(STATUS_POLL_INTERVAL) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                break;
            }

            Err(RecvTimeoutError::Timeout) => {}
        }
    }

    emulator.stop()?;

    println!("Device emulator stopped.");

    Ok(())
}
