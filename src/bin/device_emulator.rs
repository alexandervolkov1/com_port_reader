#[path = "../device_emulator.rs"]
mod device_emulator;

#[path = "../device_emulator_handle.rs"]
mod device_emulator_handle;

use std::{
    env,
    error::Error,
    io,
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::Duration,
};

use device_emulator_handle::{DeviceEmulatorHandle, DeviceEmulatorPortConfig};

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
             device_emulator <PORT> [BAUD]",
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

    if arguments.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "too many arguments; usage: \
             device_emulator <PORT> [BAUD]",
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

    let mut emulator = DeviceEmulatorHandle::start(config)?;

    println!(
        "Random walk emulator is running on \
         {port_name} at {baud_rate} baud.",
    );

    println!("Commands:");
    println!("  get");
    println!("  get <walk-id> [step]");
    println!("Press Enter to stop.");

    let (stop_sender, stop_receiver) = mpsc::channel();

    // stdin блокируется в своём потоке, а основной поток
    // одновременно следит за состоянием эмулятора.
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

    // Если поток уже аварийно завершился, stop()
    // присоединит его и вернёт исходную ошибку.
    emulator.stop()?;

    println!("Device emulator stopped.");

    Ok(())
}
