use std::time::Duration;

use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits};

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

    pub fn open(&self) -> Result<Box<dyn SerialPort>, SerialConnectionError> {
        serialport::new(&self.port_name, self.baud_rate)
            .data_bits(self.data_bits)
            .parity(self.parity)
            .stop_bits(self.stop_bits)
            .flow_control(self.flow_control)
            .timeout(Duration::from_millis(self.timeout_ms))
            .open()
            .map_err(SerialConnectionError::from)
    }
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
