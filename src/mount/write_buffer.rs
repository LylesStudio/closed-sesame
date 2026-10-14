//! Copy-on-write file buffer shared by mount backends.
//!
//! Edits accumulate in memory and are committed to the container on
//! flush/close (via `SharedVault`'s two-phase commit), so an interrupted
//! session can never destroy the previous version of a file.

/// In-memory edit buffer for one open file handle.
#[derive(Debug, Default, Clone)]
pub(crate) struct WriteBuffer {
    data: Vec<u8>,
    dirty: bool,
}

impl WriteBuffer {
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    pub(crate) fn from_bytes(data: Vec<u8>) -> Self {
        Self { data, dirty: false }
    }

    pub(crate) fn len(&self) -> u64 {
        self.data.len() as u64
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub(crate) fn mark_clean(&mut self) {
        self.dirty = false;
    }

    pub(crate) fn data(&self) -> &[u8] {
        &self.data
    }

    /// Splice `bytes` in at `offset`, zero-filling any gap and extending the
    /// buffer as needed (Normal write mode).
    pub(crate) fn write_at(&mut self, offset: u64, bytes: &[u8]) {
        let offset = offset as usize;
        if offset > self.data.len() {
            self.data.resize(offset, 0);
        }
        let end = offset + bytes.len();
        if end > self.data.len() {
            self.data.resize(end, 0);
        }
        self.data[offset..end].copy_from_slice(bytes);
        self.dirty = true;
    }

    /// Append at the current end (WriteToEOF mode).
    pub(crate) fn append(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
        self.dirty = true;
    }

    /// Write at `offset` without extending past the current length
    /// (ConstrainedIO mode). Returns the number of bytes accepted.
    pub(crate) fn write_constrained(&mut self, offset: u64, bytes: &[u8]) -> usize {
        let offset = offset as usize;
        if offset >= self.data.len() {
            return 0;
        }
        let accepted = bytes.len().min(self.data.len() - offset);
        self.data[offset..offset + accepted].copy_from_slice(&bytes[..accepted]);
        if accepted > 0 {
            self.dirty = true;
        }
        accepted
    }

    /// Truncate or zero-extend to `new_len`.
    pub(crate) fn resize(&mut self, new_len: u64) {
        self.data.resize(new_len as usize, 0);
        self.dirty = true;
    }

    /// Read a slice, clamped to the buffer bounds.
    pub(crate) fn read_at(&self, offset: u64, size: usize) -> &[u8] {
        let start = (offset as usize).min(self.data.len());
        let end = (start + size).min(self.data.len());
        &self.data[start..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_at_splices_and_extends() {
        let mut buffer = WriteBuffer::from_bytes(b"hello world".to_vec());
        buffer.write_at(6, b"WORLD!");
        assert_eq!(buffer.data(), b"hello WORLD!");
        assert!(buffer.is_dirty());
    }

    #[test]
    fn write_at_zero_fills_gaps() {
        let mut buffer = WriteBuffer::from_bytes(b"ab".to_vec());
        buffer.write_at(5, b"z");
        assert_eq!(buffer.data(), b"ab\0\0\0z");
    }

    #[test]
    fn append_grows_at_end() {
        let mut buffer = WriteBuffer::empty();
        buffer.append(b"xy");
        buffer.append(b"z");
        assert_eq!(buffer.data(), b"xyz");
    }

    #[test]
    fn constrained_write_never_extends() {
        let mut buffer = WriteBuffer::from_bytes(b"abcdef".to_vec());
        assert_eq!(buffer.write_constrained(4, b"WXYZ"), 2);
        assert_eq!(buffer.data(), b"abcdWX");
        assert_eq!(buffer.write_constrained(6, b"!"), 0);
        assert_eq!(buffer.write_constrained(100, b"!"), 0);
    }

    #[test]
    fn resize_truncates_and_extends() {
        let mut buffer = WriteBuffer::from_bytes(b"abcdef".to_vec());
        buffer.resize(3);
        assert_eq!(buffer.data(), b"abc");
        buffer.resize(6);
        assert_eq!(buffer.data(), b"abc\0\0\0");
        assert!(buffer.is_dirty());
    }

    #[test]
    fn read_at_clamps_to_bounds() {
        let buffer = WriteBuffer::from_bytes(b"abcdef".to_vec());
        assert_eq!(buffer.read_at(2, 3), b"cde");
        assert_eq!(buffer.read_at(4, 10), b"ef");
        assert_eq!(buffer.read_at(99, 4), b"");
    }

    #[test]
    fn mark_clean_resets_dirty() {
        let mut buffer = WriteBuffer::from_bytes(b"a".to_vec());
        buffer.write_at(0, b"b");
        assert!(buffer.is_dirty());
        buffer.mark_clean();
        assert!(!buffer.is_dirty());
    }
}
