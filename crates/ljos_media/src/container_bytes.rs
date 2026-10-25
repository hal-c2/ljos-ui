//! Whole-container buffering shared by remote bodies and native streams.

use std::io::{self, Read};

// DECISION: whole containers use the same 16 MiB budget as platform fetches.
// Bounded channels alone do not bound the total accumulated by their consumer.
// Elementary MJPEG/H.264 streams do not accumulate whole containers.
// Keep the streaming fetcher's existing read timing: its Read interface has
// no per-body deadline, and a shared timeout would also end live streams.
pub(crate) const LIMIT: usize = 16 * 1024 * 1024;

pub(crate) fn append(into: &mut Vec<u8>, bytes: &[u8], limit: usize) -> io::Result<()> {
    if bytes.len() > limit.saturating_sub(into.len()) || into.len() > limit {
        return Err(io::Error::other("media container exceeds byte budget"));
    }
    into.extend_from_slice(bytes);
    Ok(())
}

pub(crate) fn read(body: &mut dyn Read, into: &mut Vec<u8>, limit: usize) -> io::Result<()> {
    let mut piece = [0; 8192];
    loop {
        // Probe exactly one byte beyond the remaining budget, even at the limit.
        let want = piece.len().min(limit.saturating_sub(into.len()) + 1);
        match body.read(&mut piece[..want]) {
            Ok(0) => return Ok(()),
            Ok(n) => append(into, &piece[..n], limit)?,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endless_body_stops_at_first_excess_byte() {
        let mut body = io::repeat(7).take(1000);
        let mut all = vec![7; 2];
        assert!(read(&mut body, &mut all, 8).is_err());
        assert_eq!(body.limit(), 993);
        assert_eq!(all, [7; 2]);
        assert!(all.capacity() <= 8);
    }

    #[test]
    fn exact_budget_requires_eof_and_preserves_bytes() {
        let mut all = vec![1; 2];
        read(&mut &[2; 6][..], &mut all, 8).unwrap();
        assert_eq!(all, [1, 1, 2, 2, 2, 2, 2, 2]);
        assert!(read(&mut &[3][..], &mut all, 8).is_err());
        assert_eq!(all.len(), 8);
    }

    #[test]
    fn cumulative_writes_reject_without_extending_or_allocating() {
        let mut all = Vec::with_capacity(8);
        for _ in 0..8 {
            append(&mut all, &[1], 8).unwrap();
        }
        let capacity = all.capacity();
        for _ in 0..1000 {
            assert!(append(&mut all, &[2], 8).is_err());
            assert_eq!(all.len(), 8);
            assert_eq!(all.capacity(), capacity);
        }
    }
}
