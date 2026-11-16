// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): third-party/iohelper/src/base64.hh
// (class Base64Writer; IOHelper, Copyright EPFL/LSMS, LGPL-3.0-or-later).
// IOHelper (third-party/iohelper, v1.1.1): Copyright (©) 2010-2012, 2014 EPFL
// (Ecole Polytechnique Fédérale de Lausanne), Laboratory (LSMS - Laboratoire
// de Simulation en Mécanique des Solides). IOHelper is free software: you can
// redistribute it and/or modify it under the terms of the GNU Lesser General
// Public License as published by the Free Software Foundation, either version
// 3 of the License, or (at your option) any later version.

//! IOHelper's base64 encoder for VTK XML "binary" data arrays.
//!
//! Byte-exact port, including two properties of the original that differ
//! from what VTK's own writer produces:
//!
//! * the 4-byte header and the payload form **one** base64 stream, and the
//!   header value is `payload_bytes + 4` (the header counts itself) —
//!   reproduced: ParaView reads upstream files written this way;
//! * the header is patched by re-encoding the first two 3-byte groups; for
//!   payloads of fewer than 2 bytes the trailing group is appended a second
//!   time (e.g. an empty array encodes as `BAAAAA==AA==`), which is not a
//!   valid base64 stream. akantu-rs writes the standard encoding there
//!   (`BAAAAA==`) unless the upstream layout is requested
//!   ([`Base64Stream::new`] with `compat`); for every payload of 2 bytes or
//!   more both are identical.
//!
//! The original's output for payloads of 2 bytes or more is therefore the
//! standard encoding of `[payload_bytes + 4 as i32 LE][payload]`.
//! [`Base64Stream`] produces it in one pass while the array is written
//! (the tests compare it with the original byte-by-byte encoder,
//! [`Legacy`], on every length up to 300 bytes and around its block
//! size).

/// Encoding table (`etable` of the original).
const ETABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Decoding of one base64 character as the original's `dtable` (signed
/// `char`, `-1` for characters outside the alphabet, e.g. `=`).
fn dtable(c: u8) -> i32 {
    match c {
        b'A'..=b'Z' => i32::from(c - b'A'),
        b'a'..=b'z' => i32::from(c - b'a') + 26,
        b'0'..=b'9' => i32::from(c - b'0') + 52,
        b'+' => 62,
        b'/' => 63,
        _ => -1,
    }
}

/// Characters of a standard base64 group of 1 to 3 bytes (`=` padded).
#[inline]
fn group(b: &[u8]) -> [u8; 4] {
    let g0 = b[0];
    let g1 = b.get(1).copied().unwrap_or(0);
    let g2 = b.get(2).copied().unwrap_or(0);
    [
        ETABLE[usize::from(g0 >> 2)],
        ETABLE[usize::from(((g0 & 3) << 4) | (g1 >> 4))],
        if b.len() > 1 {
            ETABLE[usize::from(((g1 & 0xF) << 2) | (g2 >> 6))]
        } else {
            b'='
        },
        if b.len() > 2 {
            ETABLE[usize::from(g2 & 0x3F)]
        } else {
            b'='
        },
    ]
}

/// Appends the standard base64 encoding of `head` followed by `body` to
/// `out`; `head.len()` is a multiple of 3.
fn encode_into(out: &mut String, head: &[u8], body: &[u8]) {
    debug_assert!(head.len().is_multiple_of(3));
    out.reserve((head.len() + body.len()).div_ceil(3) * 4);
    // Characters are staged in a block on the stack and appended as `str`
    // (base64 characters are ASCII).
    const BLOCK: usize = 4096;
    let mut buf = [0u8; BLOCK];
    let mut n = 0;
    let flush = |buf: &[u8], out: &mut String| {
        out.push_str(std::str::from_utf8(buf).unwrap_or(""));
    };
    for g in head.as_chunks::<3>().0 {
        buf[n..n + 4].copy_from_slice(&group(g));
        n += 4;
    }
    let (groups, rest) = body.as_chunks::<3>();
    for g in groups {
        if n + 4 > BLOCK {
            flush(&buf[..n], out);
            n = 0;
        }
        let (g0, g1, g2) = (g[0], g[1], g[2]);
        buf[n] = ETABLE[usize::from(g0 >> 2)];
        buf[n + 1] = ETABLE[usize::from(((g0 & 3) << 4) | (g1 >> 4))];
        buf[n + 2] = ETABLE[usize::from(((g1 & 0xF) << 2) | (g2 >> 6))];
        buf[n + 3] = ETABLE[usize::from(g2 & 0x3F)];
        n += 4;
    }
    if !rest.is_empty() {
        if n + 4 > BLOCK {
            flush(&buf[..n], out);
            n = 0;
        }
        buf[n..n + 4].copy_from_slice(&group(rest));
        n += 4;
    }
    flush(&buf[..n], out);
}

/// Streaming encoder of one data array whose payload size is known before
/// its first byte: the header (`payload + 4` as `i32` LE) is encoded first
/// and the payload follows in blocks, so that the encoded text can be
/// written out while the array is produced. The text is the original's
/// (payloads below 2 bytes go through [`Legacy`]).
#[derive(Debug)]
pub(crate) struct Base64Stream {
    compat: bool,
    /// Payload bytes announced by [`Base64Stream::begin`].
    expected: usize,
    /// Payload bytes pushed since.
    pushed: usize,
    /// Bytes not encoded yet (header included).
    raw: Vec<u8>,
}

/// Bytes encoded at a time by [`Base64Stream`] (a multiple of 3).
const RAW_BLOCK: usize = 3 * 1024;

impl Base64Stream {
    pub(crate) fn new(compat: bool) -> Self {
        Self {
            compat,
            expected: 0,
            pushed: 0,
            raw: Vec::with_capacity(RAW_BLOCK + 16),
        }
    }

    /// Start an array of `payload` bytes.
    pub(crate) fn begin(&mut self, payload: usize) {
        self.expected = payload;
        self.pushed = 0;
        self.raw.clear();
        if payload >= 2 {
            // `static_cast<int>(nbBytes)`: dumps are far below 2 GiB per array.
            let nb = i32::try_from(payload.saturating_add(4)).unwrap_or(i32::MAX);
            self.raw.extend_from_slice(&nb.to_le_bytes());
        }
    }

    /// Push payload bytes; encoded text is appended to `out`.
    #[inline]
    pub(crate) fn push(&mut self, out: &mut String, bytes: &[u8]) {
        self.pushed += bytes.len();
        self.raw.extend_from_slice(bytes);
        if self.raw.len() >= RAW_BLOCK && self.expected >= 2 {
            encode_into(out, &[], &self.raw[..RAW_BLOCK]);
            self.raw.copy_within(RAW_BLOCK.., 0);
            self.raw.truncate(self.raw.len() - RAW_BLOCK);
        }
    }

    /// End the array: append the rest of its text to `out`. `false` if the
    /// number of bytes pushed differs from the announced one (the text is
    /// then wrong).
    pub(crate) fn end(&mut self, out: &mut String) -> bool {
        if self.expected < 2 {
            let mut l = Legacy::new(self.compat);
            l.create_header();
            l.push(&self.raw);
            out.push_str(std::str::from_utf8(l.write_header()).unwrap_or(""));
        } else {
            encode_into(out, &[], &self.raw);
        }
        self.raw.clear();
        self.pushed == self.expected
    }
}

/// The original byte-by-byte encoder, header patch included. Used for
/// payloads below 2 bytes (where the patch differs from the standard
/// encoding) and by the tests as the reference of [`Base64Stream`].
#[derive(Debug, Default)]
struct Legacy {
    /// Reproduce the header patch of tiny payloads byte for byte.
    compat: bool,
    /// First bytes of the current array (header + up to 2 payload bytes).
    head: Vec<u8>,
    /// Bytes waiting in the current 3-byte group.
    n: usize,
    igroup: [u8; 3],
    buffer: Vec<u8>,
    nb_bytes: i64,
    /// Overwrite position during the header patch (`start` ≠ −1).
    start: Option<usize>,
}

impl Legacy {
    /// Encoder; `compat` reproduces upstream's output for payloads below 2
    /// bytes.
    fn new(compat: bool) -> Self {
        Self {
            compat,
            ..Self::default()
        }
    }

    /// Start a data array: clear the buffer and push the placeholder header
    /// (`CreateHeader`).
    fn create_header(&mut self) {
        self.buffer.clear();
        self.head.clear();
        self.nb_bytes = 0;
        self.push(&0i32.to_le_bytes());
    }

    /// Push raw bytes (native little-endian representation of a value).
    fn push(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.push_byte(b);
        }
    }

    fn push_byte(&mut self, c: u8) {
        if self.start.is_none() && self.head.len() < 6 {
            self.head.push(c);
        }
        if self.n == 0 {
            self.igroup = [0; 3];
        }
        self.igroup[self.n] = c;
        self.n += 1;
        if self.n == 3 {
            self.dump_to_buffer();
        }
        self.nb_bytes += 1;
    }

    fn finish(&mut self) {
        if self.n != 0 {
            self.dump_to_buffer();
        }
    }

    fn dump_to_buffer(&mut self) {
        if self.n < 3 {
            self.igroup[2] = 0;
            if self.n < 2 {
                self.igroup[1] = 0;
            }
        }
        let [g0, g1, g2] = self.igroup;
        let mut o = [
            ETABLE[usize::from(g0 >> 2)],
            ETABLE[usize::from(((g0 & 3) << 4) | (g1 >> 4))],
            ETABLE[usize::from(((g1 & 0xF) << 2) | (g2 >> 6))],
            ETABLE[usize::from(g2 & 0x3F)],
        ];
        if self.n < 3 {
            o[3] = b'=';
            if self.n < 2 {
                o[2] = b'=';
            }
        }
        for c in o {
            match self.start {
                None => self.buffer.push(c),
                Some(s) => {
                    // The buffer holds at least two groups (8 characters)
                    // because the header alone is 4 bytes.
                    if let Some(slot) = self.buffer.get_mut(s) {
                        *slot = c;
                    }
                    self.start = Some(s + 1);
                }
            }
        }
        self.n = 0;
    }

    /// Decode four characters into three bytes; returns the decoded bytes
    /// and how many of them are meaningful (`Decode`).
    fn decode(c: [u8; 4]) -> ([u8; 3], usize) {
        let d = c.map(dtable);
        // Truncations to `u8` keep the low byte, as the `(char)` casts of
        // the original.
        let r0 = (((d[0] << 2) & 0xFC) | ((d[1] >> 4) & 0x03)) as u8;
        let r1 = (((d[1] << 4) & 0xF0) | ((d[2] >> 2) & 0x0F)) as u8;
        let r2 = (((d[2] << 6) & 0xC0) | (d[3] & 0x3F)) as u8;
        let count = if c[2] == b'=' {
            1
        } else if c[3] == b'=' {
            2
        } else {
            3
        };
        ([r0, r1, r2], count)
    }

    /// End a data array: patch the header with the byte count and return
    /// the encoded text (`WriteHeader` + `DumpToFile`).
    fn write_header(&mut self) -> &[u8] {
        if !self.compat && self.nb_bytes < 6 {
            // Standard encoding of [header = payload + 4][payload].
            let temp = i32::try_from(self.nb_bytes).unwrap_or(i32::MAX);
            let mut bytes = temp.to_le_bytes().to_vec();
            bytes.extend_from_slice(self.head.get(4..).unwrap_or(&[]));
            self.buffer.clear();
            self.n = 0;
            self.push(&bytes);
            self.finish();
            return &self.buffer;
        }
        self.finish();
        let group = |b: &[u8], i: usize| -> [u8; 4] {
            let mut g = [0u8; 4];
            for (k, slot) in g.iter_mut().enumerate() {
                *slot = b.get(i + k).copied().unwrap_or(0);
            }
            g
        };
        let ([_, b4, b5], nb) = Self::decode(group(&self.buffer, 4));
        // `static_cast<int>(nbBytes)`: dumps are far below 2 GiB per array.
        let temp = i32::try_from(self.nb_bytes).unwrap_or(i32::MAX);
        self.start = Some(0);
        self.push(&temp.to_le_bytes());
        if nb > 1 {
            self.push_byte(b4);
        }
        if nb > 2 {
            self.push_byte(b5);
        }
        self.start = None;
        self.nb_bytes = i64::from(temp);
        self.finish();
        &self.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(payload: &[u8]) -> String {
        encode_mode(payload, true)
    }

    fn encode_mode(payload: &[u8], compat: bool) -> String {
        let mut w = Base64Stream::new(compat);
        let mut out = String::new();
        w.begin(payload.len());
        w.push(&mut out, payload);
        assert!(w.end(&mut out));
        out
    }

    fn encode_legacy(payload: &[u8], compat: bool) -> String {
        let mut w = Legacy::new(compat);
        w.create_header();
        w.push(payload);
        String::from_utf8(w.write_header().to_vec()).unwrap()
    }

    #[test]
    fn stream_matches_the_original_encoder() {
        let mut x = 0x2545_F491_u32;
        let mut st = Base64Stream::new(true);
        let mut st_std = Base64Stream::new(false);
        for n in (0..300).chain([3070, 3071, 3072, 3073, 3074, 3075, 2 * 3072 + 1, 10_000]) {
            let p: Vec<u8> = (0..n)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    x.to_le_bytes()[0]
                })
                .collect();
            for (st, compat) in [(&mut st, true), (&mut st_std, false)] {
                let mut out = String::from("<");
                st.begin(n);
                for chunk in p.chunks(8) {
                    st.push(&mut out, chunk);
                }
                assert!(st.end(&mut out));
                assert_eq!(out[1..], encode_legacy(&p, compat), "{n} bytes");
            }
            // a wrong announced size is detected
            let mut out = String::new();
            st.begin(n + 1);
            st.push(&mut out, &p);
            assert!(!st.end(&mut out));
        }
    }

    #[test]
    fn header_counts_itself() {
        // 4 Int32 offsets 4, 8, 12, 16 (upstream test_dumper output).
        let mut p = Vec::new();
        for v in [4u32, 8, 12, 16] {
            p.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(encode(&p), "FAAAAAQAAAAIAAAADAAAABAAAAA=");
    }

    #[test]
    fn small_payloads_reproduce_the_header_patch() {
        assert_eq!(encode(&[]), "BAAAAA==AA==");
        assert_eq!(encode(&[1]), "BQAAAAE=AAE=");
        // From two bytes on the patch overwrites in place.
        assert_eq!(encode(&[1, 2]), "BgAAAAEC");
    }

    #[test]
    fn corrected_mode_is_standard_base64() {
        assert_eq!(encode_mode(&[], false), "BAAAAA==");
        assert_eq!(encode_mode(&[1], false), "BQAAAAE=");
        for n in 2..40u8 {
            let p: Vec<u8> = (0..n).map(|i| i.wrapping_mul(37)).collect();
            assert_eq!(encode_mode(&p, false), encode_mode(&p, true), "{n} bytes");
        }
    }

    #[test]
    fn decode_handles_padding() {
        // `=` decodes as the signed char −1 of the original table.
        assert_eq!(Legacy::decode(*b"AAE="), ([0, 1, 0x3F], 2));
        assert_eq!(Legacy::decode(*b"AA=="), ([0, 0x0F, 0xFF], 1));
        assert_eq!(Legacy::decode(*b"AAEC"), ([0, 1, 2], 3));
    }
}
