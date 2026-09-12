//! Runtime-facing emulator lifecycle and local endpoint publication.
//!
//! Publishes a local session only after server initialization. Stop/restart joins the old server and
//! clears the endpoint so requests cannot silently address a previous model instance.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::{
    app_log::LogHandle,
    device_emulator_handle::{DeviceEmulatorHandle, DeviceEmulatorPortConfig},
    protocol::virtual_instrument::{MemoryClientTransport, MemoryEndpoint},
    serial_connection::SerialPortConfig,
};

pub struct DeviceEmulatorService {
    selected_port: Option<String>,
    handle: Option<DeviceEmulatorHandle>,
    log: LogHandle,
    script_path: Option<PathBuf>,
    local_transport: Option<Arc<Mutex<Option<MemoryClientTransport>>>>,
}

impl DeviceEmulatorService {
    pub fn new(
        configured_port: Option<String>,
        configured_script_path: Option<PathBuf>,
        local_transport: Option<Arc<Mutex<Option<MemoryClientTransport>>>>,
        log: LogHandle,
    ) -> Self {
        Self {
            selected_port: configured_port.filter(|port| !port.is_empty()),
            script_path: configured_script_path.filter(|path| !path.as_os_str().is_empty()),
            local_transport,
            handle: None,
            log,
        }
    }

    pub fn is_running(&self) -> bool {
        self.handle
            .as_ref()
            .is_some_and(DeviceEmulatorHandle::is_running)
    }

    pub fn start(&mut self, serial_config: Option<&SerialPortConfig>) -> Result<(), String> {
        self.poll();

        if self
            .local_transport
            .as_ref()
            .is_some_and(|session| session.lock().unwrap_or_else(|e| e.into_inner()).is_none())
        {
            // A failed exchange may have disconnected before the server
            // thread had time to exit. Join it before publishing a new session.
            self.stop();
        }

        if self.handle.is_some() {
            return Ok(());
        }

        let Some(script_path) = self.script_path.clone() else {
            return Err("Select a Lua device model first.".to_owned());
        };

        if let Some(session) = &self.local_transport {
            let (client, transport) = MemoryEndpoint::new();
            let handle = DeviceEmulatorHandle::start_with_transport(transport, script_path)
                .map_err(|error| format!("Failed to start local device emulator: {error}"))?;
            *session.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(MemoryClientTransport::new(client));
            self.handle = Some(handle);
            return Ok(());
        }

        let Some(port_name) = self.selected_port.clone() else {
            return Err("Select an emulator COM port first.".to_owned());
        };

        let serial_config =
            serial_config.ok_or_else(|| "Serial emulator configuration is missing.".to_owned())?;
        let client_port = serial_config.port_name();

        if port_name.eq_ignore_ascii_case(client_port) {
            return Err("The application and emulator must use \
                 different COM ports."
                .to_owned());
        }

        let config = DeviceEmulatorPortConfig {
            port_name: port_name.clone(),
            baud_rate: serial_config.baud_rate(),
            data_bits: serial_config.data_bits(),
            parity: serial_config.parity(),
            stop_bits: serial_config.stop_bits(),
            flow_control: serial_config.flow_control(),
        };

        let handle = DeviceEmulatorHandle::start(config, script_path).map_err(|error| {
            format!(
                "Failed to start device emulator on \
                         {port_name}: {error}",
            )
        })?;

        self.handle = Some(handle);

        Ok(())
    }

    pub fn stop(&mut self) {
        let Some(mut handle) = self.handle.take() else {
            return;
        };

        let port_name = self.selected_port.clone();

        if let Err(error) = handle.stop() {
            let location = port_name
                .as_deref()
                .map_or(String::new(), |port| format!(" on {port}"));

            self.report_error(format!(
                "Device emulator{location} stopped \
                 with an error: {error}",
            ));
        }
        self.disconnect_local();
    }

    pub fn poll(&mut self) {
        let finished = self
            .handle
            .as_ref()
            .is_some_and(|handle| !handle.is_running());

        if !finished {
            return;
        }

        let Some(mut handle) = self.handle.take() else {
            return;
        };

        let port_name = self.selected_port.clone();

        match handle.stop() {
            Ok(()) => match port_name {
                Some(port_name) => {
                    self.log.info(format!(
                        "Device emulator stopped on \
                             {port_name}.",
                    ));
                }

                None => {
                    self.log.info("Device emulator stopped.");
                }
            },

            Err(error) => {
                let location = port_name
                    .as_deref()
                    .map_or(String::new(), |port| format!(" on {port}"));

                self.report_error(format!(
                    "Device emulator{location} stopped \
                     with an error: {error}",
                ));
            }
        }
        self.disconnect_local();
    }

    fn disconnect_local(&self) {
        if let Some(session) = &self.local_transport {
            session.lock().unwrap_or_else(|e| e.into_inner()).take();
        }
    }

    fn report_error(&self, message: impl Into<String>) {
        self.log.error(message);
    }
}

impl Drop for DeviceEmulatorService {
    fn drop(&mut self) {
        self.stop();
    }
}
