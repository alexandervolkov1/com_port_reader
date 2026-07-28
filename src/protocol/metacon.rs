const READ_COMMAND: u8 = 0x00;

pub const READ_REQUEST_LENGTH: usize = 5;

/// Request for reading one Metakon register.
///
/// Encoded packet:
///
/// `DEV CHA REG RD CRC`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadRegisterRequest {
    device: u8,
    channel: u8,
    register: u8,
}

impl ReadRegisterRequest {
    pub const fn new(device: u8, channel: u8, register: u8) -> Self {
        Self {
            device,
            channel,
            register,
        }
    }

    pub fn encode(self) -> [u8; READ_REQUEST_LENGTH] {
        let mut frame = [self.device, self.channel, self.register, READ_COMMAND, 0];

        let crc = calculate_crc(&frame[..READ_REQUEST_LENGTH - 1]);

        frame[READ_REQUEST_LENGTH - 1] = crc;

        frame
    }
}

/// Calculates the one-byte CRC used by the Metakon protocol.
///
/// The CRC is initialized with `0xFF` and calculated from every
/// byte preceding the CRC field.
pub fn calculate_crc(bytes: &[u8]) -> u8 {
    let mut crc = 0xFF_u8;

    for &byte in bytes {
        let mut data = byte;

        for _ in 0..8 {
            let feedback = (data ^ crc) & 1;

            if feedback != 0 {
                crc ^= 0x18;
            }

            crc >>= 1;
            crc |= feedback << 7;

            data >>= 1;
        }
    }

    crc
}

#[cfg(test)]
mod tests {
    use super::{ReadRegisterRequest, calculate_crc};

    #[test]
    fn matches_single_byte_reference_values() {
        assert_eq!(calculate_crc(&[0x00]), 0x35);
        assert_eq!(calculate_crc(&[0x01]), 0x6B);
        assert_eq!(calculate_crc(&[0xFF]), 0x00);
    }

    #[test]
    fn encodes_device_one_read_request() {
        let request = ReadRegisterRequest::new(
            0x01, // DEV
            0x00, // CHA
            0x01, // REG
        );

        assert_eq!(
            request.encode(),
            [
                0x01, // DEV
                0x00, // CHA
                0x01, // REG
                0x00, // RD
                0xA0, // CRC
            ],
        );
    }

    #[test]
    fn encodes_device_two_read_request() {
        let request = ReadRegisterRequest::new(
            0x02, // DEV
            0x00, // CHA
            0x01, // REG
        );

        assert_eq!(
            request.encode(),
            [
                0x02, // DEV
                0x00, // CHA
                0x01, // REG
                0x00, // RD
                0x28, // CRC
            ],
        );
    }

    #[test]
    fn includes_channel_in_crc() {
        let first_channel = ReadRegisterRequest::new(0x01, 0x00, 0x01).encode();

        let second_channel = ReadRegisterRequest::new(0x01, 0x01, 0x01).encode();

        assert_ne!(first_channel, second_channel);
        assert_ne!(first_channel[4], second_channel[4]);
    }

    #[test]
    fn includes_register_in_crc() {
        let first_register = ReadRegisterRequest::new(0x01, 0x00, 0x01).encode();

        let second_register = ReadRegisterRequest::new(0x01, 0x00, 0x02).encode();

        assert_ne!(first_register, second_register);
        assert_ne!(first_register[4], second_register[4]);
    }
}
