//! Integrity checks used by container operations.

pub(crate) fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 == 0 {
                crc >> 1
            } else {
                (crc >> 1) ^ 0xedb8_8320
            };
        }
    }
    !crc
}

pub fn verify_crc32(data: &[u8], expected: u32) -> bool {
    crc32(data) == expected
}
