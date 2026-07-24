#[path = "../device_emulator.rs"]
mod device_emulator;

#[path = "../device_emulator_handle.rs"]
mod device_emulator_handle;

use std::{
    env,
    error::Error,
    io::{self},
};

use device_emulator_handle::DeviceEmulatorHandle;

const DEFAULT_BAUD_RATE: u32 = 9_600;

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

    let mut emulator = DeviceEmulatorHandle::start(port_name.clone(), baud_rate)?;

    println!(
        "Random walk emulator is running on \
         {port_name} at {baud_rate} baud.",
    );

    println!("Commands:");
    println!("  get");
    println!("  get <walk-id> [step]");
    println!("Press Enter to stop.");

    let mut input = String::new();

    io::stdin().read_line(&mut input)?;

    emulator.stop()?;

    println!("Device emulator stopped.");

    Ok(())
}
