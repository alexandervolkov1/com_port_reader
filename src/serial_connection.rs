use std::{
    io::{Read, Write},
    time::Duration,
};

use serialport::{ClearBuffer, DataBits, FlowControl, Parity, SerialPort, StopBits};

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

pub struct SerialConnection {
    port: Box<dyn SerialPort>,
}

impl SerialConnection {
    pub fn request_f64(&mut self, command: &str) -> Result<f64, SerialConnectionError> {
        let command = command.trim();

        if command.is_empty() {
            return Err(SerialConnectionError::from(
                "Serial command cannot be empty",
            ));
        }

        if command.contains(['\r', '\n']) {
            return Err(SerialConnectionError::from(
                "Serial command cannot contain a newline",
            ));
        }

        self.port.clear(ClearBuffer::Input)?;

        self.port.write_all(command.as_bytes())?;
        self.port.write_all(b"\n")?;
        self.port.flush()?;

        let mut response = Vec::with_capacity(32);

        loop {
            let mut byte = [0_u8; 1];

            self.port.read_exact(&mut byte)?;

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

        parse_f64_response(&response)
    }
}

fn parse_f64_response(response: &[u8]) -> Result<f64, SerialConnectionError> {
    let response = std::str::from_utf8(response)
        .map_err(|error| {
            SerialConnectionError::from(format!("Serial response is not UTF-8: {error}",))
        })?
        .trim();

    if response.is_empty() {
        return Err(SerialConnectionError::from("Serial response is empty"));
    }

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
    use super::parse_f64_response;

    #[test]
    fn parses_f64_response() {
        assert_eq!(parse_f64_response(b"  -12.5\r\n"), Ok(-12.5),);
    }

    #[test]
    fn rejects_invalid_f64_response() {
        let error = parse_f64_response(b"not-a-number").unwrap_err();

        assert!(error.to_string().contains("Invalid f64 response"),);
    }

    #[test]
    fn rejects_non_finite_response() {
        let error = parse_f64_response(b"NaN").unwrap_err();

        assert!(error.to_string().contains("not finite"),);
    }
}
