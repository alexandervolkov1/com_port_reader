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
    use super::calculate_crc;

    #[test]
    fn matches_single_byte_reference_values() {
        assert_eq!(calculate_crc(&[0x00]), 0x35);
        assert_eq!(calculate_crc(&[0x01]), 0x6B);
        assert_eq!(calculate_crc(&[0xFF]), 0x00);
    }

    #[test]
    fn matches_device_one_read_request() {
        let request_without_crc = [
            0x01, // DEV
            0x00, // CHA
            0x01, // REG
            0x00, // RD
        ];

        assert_eq!(calculate_crc(&request_without_crc), 0xA0);
    }

    #[test]
    fn matches_device_two_read_request() {
        let request_without_crc = [
            0x02, // DEV
            0x00, // CHA
            0x01, // REG
            0x00, // RD
        ];

        assert_eq!(calculate_crc(&request_without_crc), 0x28);
    }

    #[test]
    fn changes_when_frame_data_changes() {
        let first = calculate_crc(&[0x01, 0x00, 0x01, 0x00]);
        let second = calculate_crc(&[0x01, 0x01, 0x01, 0x00]);

        assert_ne!(first, second);
    }
}
