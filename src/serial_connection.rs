use std::{
    io::{Read, Write},
    sync::{Arc, RwLock},
    time::Duration,
};

use serialport::{ClearBuffer, DataBits, FlowControl, Parity, SerialPort, StopBits};

use crate::connection::ConnectionId;

const MAX_RESPONSE_LENGTH: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerialPortConfig {
    port_name: String,
    baud_rate: u32,
    data_bits: DataBits,
    parity: Parity,
    stop_bits: StopBits,
    flow_control: FlowControl,
    timeout_ms: u64,
}

impl SerialPortConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        port_name: String,
        baud_rate: u32,
        data_bits: DataBits,
        parity: Parity,
        stop_bits: StopBits,
        flow_control: FlowControl,
        timeout_ms: u64,
    ) -> Self {
        Self {
            port_name,
            baud_rate,
            data_bits,
            parity,
            stop_bits,
            flow_control,
            timeout_ms,
        }
    }

    pub fn port_name(&self) -> &str {
        &self.port_name
    }

    pub fn open(&self) -> Result<SerialConnection, SerialConnectionError> {
        let port = serialport::new(&self.port_name, self.baud_rate)
            .data_bits(self.data_bits)
            .parity(self.parity)
            .stop_bits(self.stop_bits)
            .flow_control(self.flow_control)
            .timeout(Duration::from_millis(self.timeout_ms))
            .open()?;

        Ok(SerialConnection { port })
    }
}

#[derive(Clone)]
pub struct SerialConfigStore {
    connection_id: ConnectionId,
    inner: Arc<RwLock<Option<SerialPortConfig>>>,
}

impl SerialConfigStore {
    pub fn new() -> Self {
        Self::for_connection(ConnectionId::PRIMARY)
    }

    pub fn for_connection(connection_id: ConnectionId) -> Self {
        Self {
            connection_id,
            inner: Arc::new(RwLock::new(None)),
        }
    }

    pub const fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }

    pub fn set(&self, config: Option<SerialPortConfig>) {
        let mut stored_config = self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        *stored_config = config;
    }

    pub fn snapshot(&self) -> Option<SerialPortConfig> {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl Default for SerialConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SerialConnection {
    port: Box<dyn SerialPort>,
}

impl SerialConnection {
    pub fn clear_input(&mut self) -> Result<(), SerialConnectionError> {
        self.port.clear(ClearBuffer::Input)?;

        Ok(())
    }

    pub fn exchange_exact(
        &mut self,
        request: &[u8],
        response: &mut [u8],
    ) -> Result<(), SerialConnectionError> {
        if request.is_empty() {
            return Err(SerialConnectionError::from(
                "Binary serial request cannot be empty",
            ));
        }

        if response.is_empty() {
            return Err(SerialConnectionError::from(
                "Binary serial response buffer cannot be empty",
            ));
        }

        self.clear_input()?;
        self.write_all(request)?;
        self.flush()?;
        self.read_exact(response)?;

        Ok(())
    }

    pub fn request_text(&mut self, command: &str) -> Result<String, SerialConnectionError> {
        if command.trim().is_empty() {
            return Err(SerialConnectionError::from(
                "Serial command cannot be empty",
            ));
        }

        if command.contains(['\r', '\n']) {
            return Err(SerialConnectionError::from(
                "Serial command cannot contain a newline",
            ));
        }

        self.clear_input()?;
        self.write_all(command.as_bytes())?;
        self.write_all(b"\n")?;
        self.flush()?;

        let mut response = Vec::with_capacity(32);

        loop {
            let mut byte = [0_u8; 1];

            self.read_exact(&mut byte)?;

            match byte[0] {
                b'\n' => break,

                b'\r' => {}

                value => {
                    if response.len() >= MAX_RESPONSE_LENGTH {
                        return Err(SerialConnectionError::from("Serial response is too long"));
                    }

                    response.push(value);
                }
            }
        }

        parse_text_response(&response)
    }

    pub fn request_f64(&mut self, command: &str) -> Result<f64, SerialConnectionError> {
        let response = self.request_text(command.trim())?;

        parse_f64_response(&response)
    }
}

impl Read for SerialConnection {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.port.read(buffer)
    }
}

impl Write for SerialConnection {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.port.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.port.flush()
    }
}

fn parse_text_response(response: &[u8]) -> Result<String, SerialConnectionError> {
    let response = std::str::from_utf8(response)
        .map_err(|error| {
            SerialConnectionError::from(format!("Serial response is not UTF-8: {error}",))
        })?
        .trim();

    if response.is_empty() {
        return Err(SerialConnectionError::from("Serial response is empty"));
    }

    Ok(response.to_owned())
}

fn parse_f64_response(response: &str) -> Result<f64, SerialConnectionError> {
    let value = response.parse::<f64>().map_err(|error| {
        SerialConnectionError::from(format!("Invalid f64 response '{response}': {error}",))
    })?;

    if !value.is_finite() {
        return Err(SerialConnectionError::from(format!(
            "Serial response is not finite: {response}",
        )));
    }

    Ok(value)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerialConnectionError {
    message: String,
}

impl std::fmt::Display for SerialConnectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SerialConnectionError {}

impl From<serialport::Error> for SerialConnectionError {
    fn from(error: serialport::Error) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

impl From<std::io::Error> for SerialConnectionError {
    fn from(error: std::io::Error) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

impl From<String> for SerialConnectionError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<&str> for SerialConnectionError {
    fn from(message: &str) -> Self {
        Self {
            message: message.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SerialConfigStore, parse_f64_response, parse_text_response};

    use crate::connection::ConnectionId;

    #[test]
    fn parses_f64_response() {
        assert_eq!(parse_f64_response("-12.5"), Ok(-12.5),);
    }

    #[test]
    fn rejects_invalid_f64_response() {
        let error = parse_f64_response("not-a-number").unwrap_err();

        assert!(error.to_string().contains("Invalid f64 response"),);
    }

    #[test]
    fn rejects_non_finite_response() {
        let error = parse_f64_response("NaN").unwrap_err();

        assert!(error.to_string().contains("not finite"));
    }

    #[test]
    fn parses_text_response() {
        assert_eq!(
            parse_text_response(b"  heater ready  \r"),
            Ok("heater ready".to_owned()),
        );
    }

    #[test]
    fn rejects_empty_text_response() {
        let error = parse_text_response(b"   ").unwrap_err();

        assert_eq!(error.to_string(), "Serial response is empty",);
    }

    #[test]
    fn rejects_non_utf8_text_response() {
        let error = parse_text_response(&[0xff, 0xfe]).unwrap_err();

        assert!(error.to_string().contains("Serial response is not UTF-8"),);
    }

    #[test]
    fn new_config_store_uses_primary_connection() {
        let store = SerialConfigStore::new();

        assert_eq!(store.connection_id(), ConnectionId::PRIMARY,);
    }

    #[test]
    fn creates_config_store_for_connection() {
        let connection_id = ConnectionId::new(7);

        let store = SerialConfigStore::for_connection(connection_id);

        assert_eq!(store.connection_id(), connection_id,);
    }
}
