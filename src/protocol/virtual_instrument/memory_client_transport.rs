//! Late-response recovery for one outstanding local protocol request.
//!
//! A timed-out caller must not let its eventual response satisfy the next request. The adapter drains
//! a complete pending response before clearing input and starting another exchange.

use std::io::{self, Read, Write};

use super::{MemoryEndpoint, VirtualFrameDecoder, VirtualInstrumentTransport};

/// Keeps at most one request outstanding. Before the next request, consume
/// the previous response completely, even if its caller already timed out.
/// Only the local client uses this adapter; serial framing stays unchanged.
pub struct MemoryClientTransport {
    endpoint: MemoryEndpoint,
    pending: bool,
    response: Vec<u8>,
}

impl MemoryClientTransport {
    pub fn new(endpoint: MemoryEndpoint) -> Self {
        Self {
            endpoint,
            pending: false,
            response: Vec::new(),
        }
    }
}

impl Read for MemoryClientTransport {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.endpoint.read(buffer)?;
        self.response.extend_from_slice(&buffer[..count]);
        Ok(count)
    }
}

impl Write for MemoryClientTransport {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let count = self.endpoint.write(buffer)?;
        if count != 0 {
            self.pending = true;
        }
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.endpoint.flush()
    }
}

impl VirtualInstrumentTransport for MemoryClientTransport {
    fn supports_timeout_recovery(&self) -> bool {
        true
    }

    fn clear_input(&mut self) -> io::Result<()> {
        if self.pending {
            let mut decoder = VirtualFrameDecoder::new();
            decoder.push(&self.response);
            loop {
                if decoder.next_frame().map_err(io::Error::other)?.is_some() {
                    break;
                }
                let mut buffer = [0; 256];
                let count = self.read(&mut buffer)?;
                decoder.push(&buffer[..count]);
            }
            self.pending = false;
            self.response.clear();
        }
        self.endpoint.clear_input()
    }
}
