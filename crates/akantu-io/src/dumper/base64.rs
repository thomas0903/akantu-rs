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
//!   ([`Base64Writer::new`] with `compat`); for every payload of 2 bytes or
//!   more both are identical.

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

/// Streaming encoder (one instance per VTU file in upstream; reset by
/// [`Base64Writer::create_header`] for every data array).
#[derive(Debug, Default)]
pub(crate) struct Base64Writer {
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

impl Base64Writer {
    /// Encoder; `compat` reproduces upstream's output for payloads below 2
    /// bytes.
    pub(crate) fn new(compat: bool) -> Self {
        Self {
            compat,
            ..Self::default()
        }
    }

    /// Start a data array: clear the buffer and push the placeholder header
    /// (`CreateHeader`).
    pub(crate) fn create_header(&mut self) {
        self.buffer.clear();
        self.head.clear();
        self.nb_bytes = 0;
        self.push(&0i32.to_le_bytes());
    }

    /// Push raw bytes (native little-endian representation of a value).
    pub(crate) fn push(&mut self, bytes: &[u8]) {
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
    pub(crate) fn write_header(&mut self) -> &[u8] {
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
        let mut w = Base64Writer::new(compat);
        w.create_header();
        w.push(payload);
        String::from_utf8(w.write_header().to_vec()).unwrap()
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
        assert_eq!(Base64Writer::decode(*b"AAE="), ([0, 1, 0x3F], 2));
        assert_eq!(Base64Writer::decode(*b"AA=="), ([0, 0x0F, 0xFF], 1));
        assert_eq!(Base64Writer::decode(*b"AAEC"), ([0, 1, 2], 3));
    }
}
