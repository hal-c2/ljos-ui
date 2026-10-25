//! Cutting a byte stream into what a decoder takes one at a time: the
//! JPEG pictures of an MJPEG stream, the access units of an H.264 one.
//! Bytes come in as they arrive, in pieces of any size; a unit is given
//! out as soon as its end is known, and the rest waits for more.

/// How the stream is cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cut {
    /// JPEG after JPEG, anything between them (a multipart boundary)
    /// skipped.
    Jpeg,
    /// H.264 NAL units after start codes (Annex B).
    AnnexB,
    /// H.264 NAL units after a big-endian length of this many bytes.
    Length(usize),
}

#[derive(Debug)]
pub(crate) struct Splitter {
    cut: Cut,
    buf: Vec<u8>,
    /// The NAL units of the access unit being gathered.
    au: Vec<Vec<u8>>,
    /// It has a picture's slice already.
    has_slice: bool,
    scan: usize,
    entropy: bool,
    au_bytes: usize,
    limit: usize,
    nal_limit: usize,
    failed: bool,
    #[cfg(test)]
    scanned: usize,
}

impl Splitter {
    pub(crate) fn new(cut: Cut) -> Self {
        Splitter {
            cut,
            buf: Vec::new(),
            au: Vec::new(),
            has_slice: false,
            scan: 0,
            entropy: false,
            au_bytes: 0,
            limit: crate::container_bytes::LIMIT,
            nal_limit: 4096,
            failed: false,
            #[cfg(test)]
            scanned: 0,
        }
    }

    pub(crate) fn cut(&self) -> Cut {
        self.cut
    }

    // DECISION: reuse the 16 MiB container budget for all pending splitter
    // bytes, including access-unit framing. Limit an access unit to 4096 NALs
    // as well, since tiny parameter NALs otherwise amplify allocation overhead.
    // Overflow permanently fails this stream; callers report a decode error.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, &'static str> {
        if self.failed {
            return Err("media unit exceeds byte or NAL budget");
        }
        let mut out = Vec::new();
        let result = self.push_into(bytes, &mut out);
        Self::completed_before_error(out, result)
    }

    fn push_into(&mut self, bytes: &[u8], out: &mut Vec<Vec<u8>>) -> Result<(), &'static str> {
        for piece in bytes.chunks(8192) {
            if piece.len() > self.limit.saturating_sub(self.buffered_bytes()) {
                // Feed smaller pieces near the limit so complete units can drain.
                for byte in piece {
                    if self.buffered_bytes() == self.limit {
                        return self.fail();
                    }
                    self.buf.push(*byte);
                    self.extract(false, out)?;
                }
            } else {
                self.buf.extend_from_slice(piece);
                self.extract(false, out)?;
            }
        }
        Ok(())
    }

    // Failure stays latched. Return completed units first; the next call reports
    // the terminal error, even when no further bytes arrive.
    fn completed_before_error(
        out: Vec<Vec<u8>>,
        result: Result<(), &'static str>,
    ) -> Result<Vec<Vec<u8>>, &'static str> {
        if out.is_empty() {
            result?;
        }
        Ok(out)
    }

    pub(crate) fn has_failed(&self) -> bool {
        self.failed
    }

    fn buffered_bytes(&self) -> usize {
        self.buf.len() + self.au_bytes
    }

    fn fail<T>(&mut self) -> Result<T, &'static str> {
        self.failed = true;
        self.buf = Vec::new();
        self.au = Vec::new();
        self.au_bytes = 0;
        Err("media unit exceeds byte or NAL budget")
    }

    fn oversized_length(&self, n: usize) -> bool {
        if !(1..=4).contains(&n) {
            return true;
        }
        let Some(prefix) = self.buf.get(..n) else {
            return false;
        };
        let len = be(prefix);
        len > self.limit.saturating_sub(n)
    }

    fn extract(&mut self, last: bool, out: &mut Vec<Vec<u8>>) -> Result<(), &'static str> {
        match self.cut {
            Cut::Jpeg => {
                out.extend(self.jpegs());
                Ok(())
            }
            _ => self.access_units(last, out),
        }
    }

    pub(crate) fn finish(&mut self) -> Result<Vec<Vec<u8>>, &'static str> {
        if self.failed {
            return self.fail();
        }
        let mut out = Vec::new();
        let result = self.extract(true, &mut out);
        Self::completed_before_error(out, result)
    }

    fn jpegs(&mut self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        loop {
            let Some(start) = find(&self.buf, &[0xFF, 0xD8], 0) else {
                // Keep a trailing 0xFF: it may begin the next picture.
                let keep = usize::from(self.buf.last() == Some(&0xFF));
                self.buf.drain(..self.buf.len() - keep);
                return out;
            };
            match self.jpeg_end(start) {
                Some(end) => {
                    out.push(self.buf[start..end].to_vec());
                    self.buf.drain(..end);
                    self.scan = 0;
                    self.entropy = false;
                }
                None => {
                    self.buf.drain(..start);
                    self.scan = self.scan.saturating_sub(start);
                    return out;
                }
            }
        }
    }

    fn access_units(&mut self, last: bool, out: &mut Vec<Vec<u8>>) -> Result<(), &'static str> {
        loop {
            if let Cut::Length(n) = self.cut
                && self.oversized_length(n)
            {
                return self.fail();
            }
            let Some((start, end, next)) = self.next_nal(last) else {
                break;
            };
            let nal = trim_zeros(&self.buf[start..end]).to_vec();
            // The next NAL's header, after its start code or length.
            let at = next
                + match self.cut {
                    Cut::Length(n) => n,
                    _ => 3,
                };
            let invalid_next = match self.cut {
                Cut::Length(n) => self
                    .buf
                    .get(next..next + n)
                    .is_some_and(|prefix| be(prefix) > self.limit.saturating_sub(n)),
                _ => false,
            };
            let header = (!invalid_next)
                .then(|| self.buf.get(at..at + 2).map(|h| [h[0], h[1]]))
                .flatten();
            self.buf.drain(..next);
            self.scan = 0;
            if !nal.is_empty() {
                self.has_slice |= is_slice(nal[0]);
                let framed = nal.len()
                    + match self.cut {
                        Cut::Length(n) => n,
                        _ => 4,
                    };
                if self.au.len() == self.nal_limit
                    || framed > self.limit.saturating_sub(self.buffered_bytes())
                {
                    return self.fail();
                }
                self.au_bytes += framed;
                self.au.push(nal);
            }
            let starts_another = header.is_some_and(|h| begins_access_unit(h[0], h[1]));
            if self.has_slice && (starts_another || header.is_none()) {
                out.push(self.join());
            }
            if header.is_none() && !invalid_next {
                break;
            }
        }
        if last && !self.au.is_empty() {
            out.push(self.join());
        }
        Ok(())
    }

    /// The next complete NAL unit: where it starts and ends in `buf`,
    /// and where the one after it begins (its start code or length). A
    /// NAL is complete when the one after it shows two bytes of its
    /// header, so that whether it begins another access unit is known;
    /// at the `last`, the rest of the stream is one.
    fn next_nal(&mut self, last: bool) -> Option<(usize, usize, usize)> {
        match self.cut {
            Cut::AnnexB => {
                let Some(code) = find(&self.buf, &[0, 0, 1], 0) else {
                    let keep = self.buf.len().min(2);
                    self.buf.drain(..self.buf.len() - keep);
                    return None;
                };
                if code > 0 {
                    self.buf.drain(..code);
                    self.scan = self.scan.saturating_sub(code);
                }
                let start = 3;
                let from = self.scan.max(start);
                #[cfg(test)]
                {
                    self.scanned += self.buf.len().saturating_sub(from);
                }
                let next = find(&self.buf, &[0, 0, 1], from);
                self.scan = next
                    .unwrap_or_else(|| self.buf.len().saturating_sub(2))
                    .max(start);
                match next {
                    Some(next) if next + 5 <= self.buf.len() => Some((start, next, next)),
                    Some(_) => None,
                    None if last && start < self.buf.len() => {
                        Some((start, self.buf.len(), self.buf.len()))
                    }
                    None => None,
                }
            }
            Cut::Length(n) => {
                let len = be(self.buf.get(..n)?);
                let end = n + len;
                if end > self.buf.len() {
                    return None;
                }
                // An invalid next length terminates the preceding valid unit
                // even when that next NAL has no payload.
                let invalid_next = self
                    .buf
                    .get(end..end + n)
                    .is_some_and(|prefix| be(prefix) > self.limit.saturating_sub(n));
                if end + n + 2 <= self.buf.len() || last || invalid_next {
                    Some((n, end, end))
                } else {
                    None
                }
            }
            Cut::Jpeg => None,
        }
    }

    fn jpeg_end(&mut self, start: usize) -> Option<usize> {
        self.scan = self.scan.max(start + 2);
        loop {
            let i = self.scan;
            #[cfg(test)]
            {
                self.scanned += 1;
            }
            if i > self.buf.len() {
                return None;
            }
            if self.entropy {
                #[cfg(test)]
                {
                    self.scanned += self.buf.len().saturating_sub(i);
                }
                let Some(ff) = find(&self.buf, &[0xFF], i) else {
                    self.scan = self.buf.len();
                    return None;
                };
                self.scan = ff;
                let next = *self.buf.get(ff + 1)?;
                if next == 0 || (0xD0..=0xD7).contains(&next) || next == 0xFF {
                    self.scan += if next == 0xFF { 1 } else { 2 };
                    continue;
                }
                self.entropy = false;
                continue;
            }
            if *self.buf.get(i)? != 0xFF {
                return Some(i);
            }
            let marker = *self.buf.get(i + 1)?;
            match marker {
                0xFF => self.scan += 1,
                0xD9 => return Some(i + 2),
                0xD0..=0xD7 | 0x01 => self.scan += 2,
                _ => {
                    let len = be(self.buf.get(i + 2..i + 4)?);
                    self.scan += 2 + len;
                    self.entropy = marker == 0xDA;
                }
            }
        }
    }

    fn join(&mut self) -> Vec<u8> {
        self.has_slice = false;
        self.au_bytes = 0;
        let mut out = Vec::new();
        for nal in self.au.drain(..) {
            match self.cut {
                Cut::Length(n) => out.extend_from_slice(&(nal.len() as u64).to_be_bytes()[8 - n..]),
                _ => out.extend_from_slice(&[0, 0, 0, 1]),
            }
            out.extend_from_slice(&nal);
        }
        out
    }
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + from)
}

fn be(bytes: &[u8]) -> usize {
    bytes.iter().fold(0, |n, b| (n << 8) | usize::from(*b))
}

/// Without the zero bytes a start code of four leaves at its end.
fn trim_zeros(nal: &[u8]) -> &[u8] {
    let end = nal.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1);
    &nal[..end]
}

fn is_slice(header: u8) -> bool {
    matches!(header & 0x1F, 1 | 5)
}

/// Whether a NAL unit whose first two bytes are `h0`, `h1` begins an
/// access unit after one that has a slice (H.264 7.4.1.2.3): a delimiter,
/// parameter set or SEI, or the first slice of a picture.
fn begins_access_unit(h0: u8, h1: u8) -> bool {
    match h0 & 0x1F {
        6..=9 | 14..=18 => true,
        // first_mb_in_slice is 0: its Exp-Golomb code is the one bit 1.
        1 | 5 => h1 & 0x80 != 0,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small(cut: Cut) -> Splitter {
        let mut s = Splitter::new(cut);
        s.limit = 128;
        s.nal_limit = 4;
        s
    }

    #[test]
    fn incomplete_units_are_bounded_and_scanned_linearly() {
        for (cut, prefix) in [
            (Cut::Jpeg, &b"\xff\xd8\xff\xda\x00\x02"[..]),
            (Cut::AnnexB, &b"\x00\x00\x01\x67\x01"[..]),
        ] {
            let mut s = small(cut);
            s.push(prefix).unwrap();
            for _ in prefix.len()..128 {
                s.push(&[1]).unwrap();
            }
            assert!(s.scanned < 512, "{}", s.scanned);
            assert!(s.push(&[1]).is_err());
            assert!(s.buf.is_empty());
            assert!(s.au.is_empty());
            assert!(s.finish().is_err());
            assert!(s.push(&jpeg(1)).is_err());
        }
    }

    #[test]
    fn declared_lengths_fail_before_payload_allocation() {
        for n in [1, 2, 3, 4] {
            let mut s = small(Cut::Length(n));
            assert!(s.push(&vec![255; n]).is_err());
            assert_eq!(s.buf.capacity(), 0);
        }
        let mut s = small(Cut::Length(4));
        assert!(s.push(&[0, 0, 0, 2, 0x67, 1, 255, 255, 255, 255]).is_err());
    }

    #[test]
    fn access_units_bound_both_nal_count_and_framed_bytes() {
        for nal in [vec![0x67, 1], vec![0x67; 60]] {
            let mut s = small(Cut::AnnexB);
            let mut failed = false;
            for _ in 0..10 {
                let mut bytes = vec![0, 0, 1];
                bytes.extend(&nal);
                if s.push(&bytes).is_err() {
                    failed = true;
                    break;
                }
            }
            assert!(failed);
            assert_eq!(s.au_bytes, 0);
        }
    }

    #[test]
    fn a_large_write_of_complete_pictures_can_exceed_the_pending_budget() {
        let mut s = small(Cut::Jpeg);
        let pictures: Vec<_> = (0..100).map(jpeg).collect();
        let bytes = pictures.concat();
        assert!(bytes.len() > s.limit);
        assert_eq!(s.push(&bytes).unwrap(), pictures);
        assert_eq!(s.buffered_bytes(), 0);
    }

    fn before_error(cut: Cut, bytes: &[u8], split: usize) -> Vec<Vec<u8>> {
        let mut s = small(cut);
        let mut out = Vec::new();
        for part in [&bytes[..split], &bytes[split..]] {
            match s.push(part) {
                Ok(units) => out.extend(units),
                Err(_) => assert!(s.has_failed()),
            }
        }
        assert!(s.finish().is_err());
        assert!(s.push(&[]).is_err());
        assert_eq!(s.buffered_bytes(), 0);
        out
    }

    #[test]
    fn completed_jpeg_scans_survive_later_overflow_for_every_split() {
        let mut picture = jpeg(1);
        picture.truncate(picture.len() - 2);
        // Another scan, marker fill, TEM, stuffing and every restart marker.
        picture.extend([0xff, 0x01, 0xff, 0xff, 0xda, 0, 2, 1, 0xff, 0]);
        for marker in 0xd0..=0xd7 {
            picture.extend([0xff, marker, 1]);
        }
        picture.extend([0xff, 0xd9]);
        let mut bytes = picture.clone();
        bytes.extend([0xff, 0xd8, 0xff, 0xda, 0, 2]);
        bytes.extend([1; 129]);
        for split in 0..=bytes.len() {
            assert_eq!(
                before_error(Cut::Jpeg, &bytes, split).as_slice(),
                std::slice::from_ref(&picture)
            );
        }
    }

    fn framed(cut: Cut, nal: &[u8]) -> Vec<u8> {
        let mut bytes = match cut {
            Cut::Length(n) => (nal.len() as u32).to_be_bytes()[4 - n..].to_vec(),
            _ => vec![0, 0, 0, 1],
        };
        bytes.extend(nal);
        bytes
    }

    #[test]
    fn completed_access_units_survive_byte_nal_and_declared_length_overflow() {
        for cut in [
            Cut::AnnexB,
            Cut::Length(1),
            Cut::Length(2),
            Cut::Length(3),
            Cut::Length(4),
        ] {
            let picture = framed(cut, &[0x65, 0x80, 1]);
            for oversized in [false, true] {
                let mut bytes = picture.clone();
                if oversized {
                    let header = if cut == Cut::AnnexB { 0x67 } else { 0x41 };
                    bytes.extend(framed(cut, &[header; 129]));
                } else {
                    for _ in 0..6 {
                        bytes.extend(framed(cut, &[0x67, 1]));
                    }
                }
                for split in 0..=bytes.len() {
                    assert_eq!(
                        before_error(cut, &bytes, split).as_slice(),
                        std::slice::from_ref(&picture),
                        "{cut:?} split {split}"
                    );
                }
            }
        }
    }

    #[test]
    fn exact_byte_and_nal_limits_are_accepted() {
        let mut picture = vec![0xff, 0xd8, 0xff, 0xda, 0, 2];
        picture.resize(126, 1);
        picture.extend([0xff, 0xd9]);
        assert_eq!(small(Cut::Jpeg).push(&picture).unwrap(), [picture]);
        for cut in [
            Cut::AnnexB,
            Cut::Length(1),
            Cut::Length(2),
            Cut::Length(3),
            Cut::Length(4),
        ] {
            let mut bytes = Vec::new();
            for _ in 0..3 {
                bytes.extend(framed(cut, &[0x67, 1]));
            }
            bytes.extend(framed(cut, &[0x65, 0x80]));
            let mut s = small(cut);
            s.limit = bytes.len();
            let mut out = Vec::new();
            for byte in &bytes {
                out.extend(s.push(&[*byte]).unwrap());
            }
            out.extend(s.finish().unwrap());
            assert_eq!(out, [bytes]);
        }
    }

    fn jpeg(n: u8) -> Vec<u8> {
        // SOI, an APP0 of 4 bytes, SOS of length 2, data with stuffing
        // and a restart, EOI.
        vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0, 4, n, n, 0xFF, 0xDA, 0, 2, 1, 0xFF, 0, 2, 0xFF, 0xD0, 3,
            0xFF, 0xD9,
        ]
    }

    #[test]
    fn jpegs_come_out_whole_whatever_the_pieces() {
        let mut stream = b"--boundary\r\n\r\n".to_vec();
        stream.extend(jpeg(1));
        stream.extend(b"\r\n--boundary\r\n");
        stream.extend(jpeg(2));
        for size in [1, 2, 5, 64] {
            let mut s = small(Cut::Jpeg);
            let mut got = Vec::new();
            for piece in stream.chunks(size) {
                got.extend(s.push(piece).unwrap());
            }
            got.extend(s.finish().unwrap());
            assert_eq!(got, [jpeg(1), jpeg(2)], "in pieces of {size}");
        }
    }

    #[test]
    fn a_jpeg_is_given_once_its_end_is_there() {
        let mut s = small(Cut::Jpeg);
        let j = jpeg(7);
        assert!(s.push(&j[..j.len() - 1]).unwrap().is_empty());
        assert_eq!(s.push(&j[j.len() - 1..]).unwrap(), [j]);
    }

    /// SPS, PPS, an IDR slice, then two pictures of one slice each, and
    /// one of two slices.
    fn annexb() -> Vec<u8> {
        let mut s = Vec::new();
        for nal in [
            &[0x67, 1][..],
            &[0x68, 2],
            &[0x65, 0x88, 3],
            &[0x41, 0x9A, 4],
            &[0x41, 0x9A, 5],
            &[0x41, 0x9A, 6],
            &[0x41, 0x40, 7],
        ] {
            s.extend([0, 0, 0, 1]);
            s.extend(nal);
        }
        s
    }

    fn au(nals: &[&[u8]]) -> Vec<u8> {
        nals.iter()
            .flat_map(|n| [&[0, 0, 0, 1][..], n].concat())
            .collect()
    }

    #[test]
    fn access_units_are_cut_at_their_first_slice() {
        let want = [
            au(&[&[0x67, 1], &[0x68, 2], &[0x65, 0x88, 3]]),
            au(&[&[0x41, 0x9A, 4]]),
            au(&[&[0x41, 0x9A, 5]]),
            au(&[&[0x41, 0x9A, 6], &[0x41, 0x40, 7]]),
        ];
        for size in [1, 3, 100] {
            let mut s = small(Cut::AnnexB);
            let mut got = Vec::new();
            for piece in annexb().chunks(size) {
                got.extend(s.push(piece).unwrap());
            }
            // All but the last is known before the end.
            assert_eq!(got.len(), 3, "in pieces of {size}");
            got.extend(s.finish().unwrap());
            assert_eq!(got, want, "in pieces of {size}");
        }
    }

    #[test]
    fn length_prefixed_units_keep_their_framing() {
        for n in 1..=4 {
            let mut stream = Vec::new();
            for nal in [&[0x65, 0x88, 3][..], &[0x41, 0x9A, 4]] {
                stream.extend((nal.len() as u32).to_be_bytes()[4 - n..].iter());
                stream.extend(nal);
            }
            for size in 1..=stream.len() {
                let mut s = small(Cut::Length(n));
                let mut got = Vec::new();
                for part in stream.chunks(size) {
                    got.extend(s.push(part).unwrap());
                }
                got.extend(s.finish().unwrap());
                assert_eq!(got, [stream[..n + 3].to_vec(), stream[n + 3..].to_vec()]);
            }
        }
    }
}
