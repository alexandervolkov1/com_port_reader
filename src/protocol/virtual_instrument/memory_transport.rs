//! Paired in-memory byte-stream endpoints for the normal emulator workflow.
//!
//! Read deadlines and disconnection behavior let the same framing server run without physical or
//! virtual COM ports. Endpoints are session-scoped and are replaced on restart.

use std::{
    collections::VecDeque,
    io,
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    time::Duration,
};

use super::client::VirtualInstrumentTransport;

const DEFAULT_READ_TIMEOUT: Duration = Duration::from_millis(100);

/// One endpoint of an in-process, ordered byte-stream connection.
///
/// Each write is delivered as one block to the peer, while reads may consume
/// only part of that block. This mirrors the observable byte-stream behavior
/// needed by the virtual-instrument protocol without emulating serial settings.
pub struct MemoryEndpoint {
    incoming: Receiver<Vec<u8>>,
    outgoing: Sender<Vec<u8>>,
    read_buffer: VecDeque<u8>,
    read_timeout: Duration,
}

impl MemoryEndpoint {
    pub fn with_timeout(read_timeout: Duration) -> (Self, Self) {
        let (first_sender, second_receiver) = mpsc::channel();
        let (second_sender, first_receiver) = mpsc::channel();

        (
            Self {
                incoming: first_receiver,
                outgoing: first_sender,
                read_buffer: VecDeque::new(),
                read_timeout,
            },
            Self {
                incoming: second_receiver,
                outgoing: second_sender,
                read_buffer: VecDeque::new(),
                read_timeout,
            },
        )
    }

    pub fn new() -> (Self, Self) {
        Self::with_timeout(DEFAULT_READ_TIMEOUT)
    }

    fn fill_read_buffer(&mut self) -> io::Result<()> {
        let result = self.incoming.recv_timeout(self.read_timeout);
        match result {
            Ok(bytes) => {
                self.read_buffer.extend(bytes);
                Ok(())
            }
            Err(RecvTimeoutError::Timeout) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "memory transport read timed out",
            )),
            Err(RecvTimeoutError::Disconnected) => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "memory transport peer disconnected",
            )),
        }
    }
}

impl std::io::Read for MemoryEndpoint {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.read_buffer.is_empty() {
            self.fill_read_buffer()?;
        }

        let length = buffer.len().min(self.read_buffer.len());
        for byte in &mut buffer[..length] {
            *byte = self.read_buffer.pop_front().expect("length was checked");
        }
        Ok(length)
    }
}

impl std::io::Write for MemoryEndpoint {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        self.outgoing.send(buffer.to_vec()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "memory transport peer disconnected",
            )
        })?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl VirtualInstrumentTransport for MemoryEndpoint {
    fn clear_input(&mut self) -> io::Result<()> {
        self.read_buffer.clear();
        while self.incoming.try_recv().is_ok() {}
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        thread,
        time::Duration,
    };

    use super::MemoryEndpoint;

    #[test]
    fn delivers_client_to_server_bytes_in_order() {
        let (mut client, mut server) = MemoryEndpoint::new();
        client.write_all(b"first").unwrap();
        client.write_all(b"second").unwrap();

        let mut buffer = [0; 11];
        server.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, b"firstsecond");
    }

    #[test]
    fn delivers_server_to_client_bytes_in_order() {
        let (mut client, mut server) = MemoryEndpoint::new();
        server.write_all(b"response").unwrap();

        let mut buffer = [0; 8];
        client.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, b"response");
    }

    #[test]
    fn supports_partial_reads_and_large_blocks() {
        let (mut first, mut second) = MemoryEndpoint::new();
        let payload = vec![0xA5; 8192];
        first.write_all(&payload).unwrap();

        let mut received = Vec::new();
        let mut chunk = [0; 37];
        while received.len() < payload.len() {
            let count = second.read(&mut chunk).unwrap();
            received.extend_from_slice(&chunk[..count]);
        }
        assert_eq!(received, payload);
    }

    #[test]
    fn times_out_when_peer_has_not_written() {
        let (_first, mut second) = MemoryEndpoint::with_timeout(Duration::from_millis(1));
        let error = second.read(&mut [0; 1]).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    }

    #[test]
    fn buffered_bytes_survive_timeout_and_peer_disconnect() {
        let (mut first, mut second) = MemoryEndpoint::with_timeout(Duration::from_millis(1));
        first.write_all(b"abc").unwrap();
        let mut prefix = [0; 1];
        second.read_exact(&mut prefix).unwrap();
        assert_eq!(&prefix, b"a");
        drop(first);
        let mut remaining = [0; 2];
        second.read_exact(&mut remaining).unwrap();
        assert_eq!(&remaining, b"bc");
        assert_eq!(
            second.read(&mut prefix).unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn clear_input_discards_partial_and_queued_blocks_and_timeout_is_recoverable() {
        use crate::protocol::virtual_instrument::VirtualInstrumentTransport;
        let (mut first, mut second) = MemoryEndpoint::with_timeout(Duration::from_millis(1));
        first.write_all(b"abc").unwrap();
        first.write_all(b"def").unwrap();
        second.read_exact(&mut [0; 1]).unwrap();
        second.clear_input().unwrap();
        assert_eq!(
            second.read(&mut [0; 1]).unwrap_err().kind(),
            std::io::ErrorKind::TimedOut
        );
        first.write_all(b"new").unwrap();
        let mut response = [0; 3];
        second.read_exact(&mut response).unwrap();
        assert_eq!(&response, b"new");
    }

    #[test]
    fn reports_disconnect_for_reads_and_writes() {
        let (mut first, second) = MemoryEndpoint::new();
        drop(second);

        let read_error = first.read(&mut [0; 1]).unwrap_err();
        assert_eq!(read_error.kind(), std::io::ErrorKind::UnexpectedEof);

        let write_error = first.write(b"late").unwrap_err();
        assert_eq!(write_error.kind(), std::io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn supports_repeated_request_response_traffic() {
        let (mut client, mut server) = MemoryEndpoint::new();
        let server_thread = thread::spawn(move || {
            for _ in 0..3 {
                let mut request = [0; 4];
                server.read_exact(&mut request).unwrap();
                server.write_all(&request).unwrap();
            }
        });

        for request in [b"one!", b"two?", b"tri#"] {
            client.write_all(request).unwrap();
            let mut response = [0; 4];
            client.read_exact(&mut response).unwrap();
            assert_eq!(&response, request);
        }
        server_thread.join().unwrap();
    }
}
