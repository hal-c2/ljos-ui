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
}

impl Splitter {
    pub(crate) fn new(cut: Cut) -> Self {
        Splitter {
            cut,
            buf: Vec::new(),
            au: Vec::new(),
            has_slice: false,
        }
    }

    pub(crate) fn cut(&self) -> Cut {
        self.cut
    }

    /// Takes `bytes` and gives the units they complete.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(bytes);
        match self.cut {
            Cut::Jpeg => self.jpegs(),
            _ => self.access_units(false),
        }
    }

    /// The stream ended: gives what is left as a last unit.
    pub(crate) fn finish(&mut self) -> Vec<Vec<u8>> {
        match self.cut {
            Cut::Jpeg => self.jpegs(),
            _ => self.access_units(true),
        }
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
            match jpeg_end(&self.buf, start) {
                Some(end) => {
                    out.push(self.buf[start..end].to_vec());
                    self.buf.drain(..end);
                }
                None => {
                    self.buf.drain(..start);
                    return out;
                }
            }
        }
    }

    fn access_units(&mut self, last: bool) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        while let Some((start, end, next)) = self.next_nal(last) {
            let nal = trim_zeros(&self.buf[start..end]).to_vec();
            // The next NAL's header, after its start code or length.
            let at = next
                + match self.cut {
                    Cut::Length(n) => n,
                    _ => 3,
                };
            let header = self.buf.get(at..at + 2).map(|h| [h[0], h[1]]);
            self.buf.drain(..next);
            if !nal.is_empty() {
                self.has_slice |= is_slice(nal[0]);
                self.au.push(nal);
            }
            let starts_another = header.is_some_and(|h| begins_access_unit(h[0], h[1]));
            if self.has_slice && (starts_another || header.is_none()) {
                out.push(self.join());
            }
            if header.is_none() {
                break;
            }
        }
        if last && !self.au.is_empty() {
            out.push(self.join());
        }
        out
    }

    /// The next complete NAL unit: where it starts and ends in `buf`,
    /// and where the one after it begins (its start code or length). A
    /// NAL is complete when the one after it shows two bytes of its
    /// header, so that whether it begins another access unit is known;
    /// at the `last`, the rest of the stream is one.
    fn next_nal(&self, last: bool) -> Option<(usize, usize, usize)> {
        match self.cut {
            Cut::AnnexB => {
                let code = find(&self.buf, &[0, 0, 1], 0)?;
                let start = code + 3;
                match find(&self.buf, &[0, 0, 1], start) {
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
                if end + n + 2 <= self.buf.len() || last {
                    Some((n, end, end))
                } else {
                    None
                }
            }
            Cut::Jpeg => None,
        }
    }

    fn join(&mut self) -> Vec<u8> {
        self.has_slice = false;
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

/// Where the JPEG beginning at `start` ends, once all of it is there.
fn jpeg_end(buf: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 2;
    loop {
        // A marker, after any fill bytes.
        while *buf.get(i)? == 0xFF && *buf.get(i + 1)? == 0xFF {
            i += 1;
        }
        if *buf.get(i)? != 0xFF {
            // Not a JPEG after all: what follows its SOI is no marker.
            return Some(i);
        }
        let marker = *buf.get(i + 1)?;
        match marker {
            0xD9 => return Some(i + 2),
            0xD0..=0xD7 | 0x01 => i += 2,
            _ => {
                let len = be(buf.get(i + 2..i + 4)?);
                i += 2 + len;
                if marker == 0xDA {
                    // Entropy-coded data, up to the next marker that is
                    // neither stuffing nor a restart.
                    loop {
                        let ff = find(buf, &[0xFF], i)?;
                        let next = *buf.get(ff + 1)?;
                        if next == 0 || (0xD0..=0xD7).contains(&next) || next == 0xFF {
                            i = ff + if next == 0xFF { 1 } else { 2 };
                            continue;
                        }
                        i = ff;
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            let mut s = Splitter::new(Cut::Jpeg);
            let mut got = Vec::new();
            for piece in stream.chunks(size) {
                got.extend(s.push(piece));
            }
            got.extend(s.finish());
            assert_eq!(got, [jpeg(1), jpeg(2)], "in pieces of {size}");
        }
    }

    #[test]
    fn a_jpeg_is_given_once_its_end_is_there() {
        let mut s = Splitter::new(Cut::Jpeg);
        let j = jpeg(7);
        assert!(s.push(&j[..j.len() - 1]).is_empty());
        assert_eq!(s.push(&j[j.len() - 1..]), [j]);
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
            let mut s = Splitter::new(Cut::AnnexB);
            let mut got = Vec::new();
            for piece in annexb().chunks(size) {
                got.extend(s.push(piece));
            }
            // All but the last is known before the end.
            assert_eq!(got.len(), 3, "in pieces of {size}");
            got.extend(s.finish());
            assert_eq!(got, want, "in pieces of {size}");
        }
    }

    #[test]
    fn length_prefixed_units_keep_their_framing() {
        let mut stream = Vec::new();
        for nal in [&[0x65, 0x88, 3][..], &[0x41, 0x9A, 4]] {
            stream.extend([0, 0, 0, nal.len() as u8]);
            stream.extend(nal);
        }
        let mut s = Splitter::new(Cut::Length(4));
        let mut got = s.push(&stream);
        got.extend(s.finish());
        assert_eq!(got, [stream[..7].to_vec(), stream[7..].to_vec()]);
    }
}
