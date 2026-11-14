// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/synchronizer/communication_buffer.hh,
// communication_buffer_inline_impl.hh.

//! Byte buffers with typed packing (upstream `CommunicationBuffer`).
//!
//! Encoding: native little-endian fixed-width values, no padding, no
//! headers — sizes are agreed upon beforehand through
//! `DataAccessor::nb_data` (as upstream). All supported targets are
//! little-endian (x86_64, aarch64); a big-endian target would fail at build
//! time via the assertion below.

use akantu_core::{Element, ElementType, Error, GhostType, NodeFlag, Result};

#[cfg(target_endian = "big")]
compile_error!("akantu-comm assumes a little-endian target");

/// Types that can be packed into a [`CommunicationBuffer`].
pub trait Packable: Sized {
    /// Packed size in bytes.
    const SIZE: usize;
    /// Append to `out`.
    fn pack(&self, out: &mut Vec<u8>);
    /// Read from the first `SIZE` bytes of `bytes`.
    fn unpack(bytes: &[u8]) -> Result<Self>;

    /// Append every value of `vs` to `out`, in order (the bytes of
    /// [`Packable::pack`] for each value). Numeric types override it with a
    /// bulk copy (the per-value appends ran at ~3 GB/s).
    fn pack_all(vs: &[Self], out: &mut Vec<u8>) {
        out.reserve(vs.len().saturating_mul(Self::SIZE));
        for v in vs {
            v.pack(out);
        }
    }

    /// Fill `out` from `bytes`, in order ([`Packable::unpack`] of each
    /// value); an error if `bytes` holds fewer than `out.len() * SIZE`
    /// bytes or a value is invalid.
    fn unpack_all(bytes: &[u8], out: &mut [Self]) -> Result<()> {
        let mut rest = bytes;
        for o in out {
            *o = Self::unpack(rest)?;
            rest = rest.get(Self::SIZE..).unwrap_or(&[]);
        }
        Ok(())
    }
}

macro_rules! packable_num {
    ($($t:ty),*) => {$(
        impl Packable for $t {
            const SIZE: usize = std::mem::size_of::<$t>();
            #[inline]
            fn pack(&self, out: &mut Vec<u8>) {
                out.extend_from_slice(&self.to_le_bytes());
            }
            #[inline]
            fn unpack(bytes: &[u8]) -> Result<Self> {
                let arr: [u8; std::mem::size_of::<$t>()] = bytes
                    .get(..Self::SIZE)
                    .and_then(|b| b.try_into().ok())
                    .ok_or_else(|| Error::Communication("buffer underflow".into()))?;
                Ok(<$t>::from_le_bytes(arr))
            }
            fn pack_all(vs: &[Self], out: &mut Vec<u8>) {
                const N: usize = std::mem::size_of::<$t>();
                let start = out.len();
                // A slice never spans more than `isize::MAX` bytes.
                out.resize(start + vs.len() * N, 0);
                for (d, v) in out[start..].chunks_exact_mut(N).zip(vs) {
                    d.copy_from_slice(&v.to_le_bytes());
                }
            }
            fn unpack_all(bytes: &[u8], out: &mut [Self]) -> Result<()> {
                const N: usize = std::mem::size_of::<$t>();
                let src = bytes
                    .get(..out.len() * N)
                    .ok_or_else(|| Error::Communication("buffer underflow".into()))?;
                for (o, s) in out.iter_mut().zip(src.chunks_exact(N)) {
                    let mut a = [0u8; N];
                    a.copy_from_slice(s);
                    *o = <$t>::from_le_bytes(a);
                }
                Ok(())
            }
        }
    )*};
}
packable_num!(u8, i8, u16, i16, u32, i32, u64, i64, f32, f64);

impl Packable for bool {
    const SIZE: usize = 1;
    fn pack(&self, out: &mut Vec<u8>) {
        out.push(u8::from(*self));
    }
    fn unpack(bytes: &[u8]) -> Result<Self> {
        match bytes.first() {
            Some(0) => Ok(false),
            Some(1) => Ok(true),
            Some(_) => Err(Error::Communication("invalid bool in buffer".into())),
            None => Err(Error::Communication("buffer underflow".into())),
        }
    }
}

impl Packable for NodeFlag {
    const SIZE: usize = 1;
    fn pack(&self, out: &mut Vec<u8>) {
        out.push(self.bits());
    }
    fn unpack(bytes: &[u8]) -> Result<Self> {
        Ok(NodeFlag(u8::unpack(bytes)?))
    }
}

impl Packable for Element {
    /// `type: u8, ghost: u8, index: i32` (`ElementNull` = `(0, 0, -1)`).
    const SIZE: usize = 6;
    fn pack(&self, out: &mut Vec<u8>) {
        out.push(self.ty() as u8);
        out.push(self.ghost_type() as u8);
        self.index().pack(out);
    }
    fn unpack(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < Self::SIZE {
            return Err(Error::Communication("buffer underflow".into()));
        }
        let ty = ElementType::from_index(bytes[0] as usize)
            .ok_or_else(|| Error::Communication("invalid element type in buffer".into()))?;
        let ghost_type = match bytes[1] {
            0 => GhostType::NotGhost,
            1 => GhostType::Ghost,
            _ => return Err(Error::Communication("invalid ghost type in buffer".into())),
        };
        // Remote data: only the canonical null or a valid element is accepted.
        Element::from_raw_parts(ty, i32::unpack(&bytes[2..])?, ghost_type)
            .map_err(|e| Error::Communication(format!("invalid element in buffer: {e}")))
    }
}

/// Growable byte buffer with a read cursor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommunicationBuffer {
    data: Vec<u8>,
    read: usize,
}

impl CommunicationBuffer {
    /// Empty buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Buffer with reserved capacity (bytes), as computed by `nb_data`.
    #[must_use]
    pub fn with_capacity(bytes: usize) -> Self {
        Self {
            data: Vec::with_capacity(bytes),
            read: 0,
        }
    }

    /// Wrap received bytes.
    #[must_use]
    pub fn from_bytes(data: Vec<u8>) -> Self {
        Self { data, read: 0 }
    }

    /// Raw bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Consume into raw bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }

    /// Total size in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// `true` if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Bytes not yet unpacked.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.data.len() - self.read
    }

    /// Clear content and cursor (upstream `reset` + `resize(0)`).
    pub fn clear(&mut self) {
        self.data.clear();
        self.read = 0;
    }

    /// Rewind the read cursor (upstream `reset`).
    pub fn rewind(&mut self) {
        self.read = 0;
    }

    /// Pack one value (upstream `operator<<`).
    pub fn pack<T: Packable>(&mut self, v: &T) {
        v.pack(&mut self.data);
    }

    /// Pack a slice of values.
    pub fn pack_slice<T: Packable>(&mut self, vs: &[T]) {
        T::pack_all(vs, &mut self.data);
    }

    /// Unpack one value (upstream `operator>>`).
    pub fn unpack<T: Packable>(&mut self) -> Result<T> {
        let v = T::unpack(&self.data[self.read..])?;
        self.read += T::SIZE;
        Ok(v)
    }

    /// Unpack into a slice (on error the read position is unchanged).
    pub fn unpack_slice<T: Packable>(&mut self, out: &mut [T]) -> Result<()> {
        T::unpack_all(&self.data[self.read..], out)?;
        self.read += out.len() * T::SIZE;
        Ok(())
    }

    /// Pack a length-prefixed sequence (upstream `operator<<` for
    /// `std::vector<T>`: a `size_t` (here `u64`) count, then the items).
    pub fn pack_vec<T: Packable>(&mut self, vs: &[T]) {
        self.pack(&u64::try_from(vs.len()).unwrap_or(u64::MAX));
        self.pack_slice(vs);
    }

    /// Unpack a length-prefixed sequence (checked against the bytes left:
    /// a corrupt count is an error, never a huge allocation).
    pub fn unpack_vec<T: Packable>(&mut self) -> Result<Vec<T>> {
        let n = usize::try_from(self.unpack::<u64>()?)
            .map_err(|_| Error::Communication("sequence length overflow".into()))?;
        if n.checked_mul(T::SIZE).is_none_or(|b| b > self.remaining()) {
            return Err(Error::Communication(format!(
                "sequence of {n} items exceeds the {} bytes left in the buffer",
                self.remaining()
            )));
        }
        (0..n).map(|_| self.unpack()).collect()
    }

    /// Pack a length-prefixed UTF-8 string (upstream `std::string`).
    pub fn pack_str(&mut self, s: &str) {
        self.pack_vec(s.as_bytes());
    }

    /// Unpack a length-prefixed UTF-8 string.
    pub fn unpack_string(&mut self) -> Result<String> {
        String::from_utf8(self.unpack_vec::<u8>()?)
            .map_err(|_| Error::Communication("invalid UTF-8 string in buffer".into()))
    }

    /// Packed size of a length-prefixed sequence of `n` items (upstream
    /// `sizeInBuffer`), for `DataAccessor::nb_data`. Saturates at
    /// `usize::MAX` on overflow (like [`crate::accessor::record_bytes`]),
    /// which the size checks then reject as beyond the message limit.
    #[must_use]
    pub fn size_of_vec<T: Packable>(n: usize) -> usize {
        n.checked_mul(T::SIZE)
            .and_then(|b| b.checked_add(u64::SIZE))
            .unwrap_or(usize::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bulk paths of the numeric types produce and
    /// read exactly the bytes of the per-value `pack`/`unpack`.
    #[test]
    fn bulk_pack_equals_per_value_pack() {
        fn check<T: Packable + Copy + PartialEq + std::fmt::Debug>(vs: &[T], zero: T) {
            let mut one = Vec::new();
            for v in vs {
                v.pack(&mut one);
            }
            let mut bulk = vec![0xAB]; // appended after existing bytes
            T::pack_all(vs, &mut bulk);
            assert_eq!(&bulk[1..], &one[..]);
            let mut back = vec![zero; vs.len()];
            T::unpack_all(&one, &mut back).unwrap();
            assert_eq!(back, vs);
            // Too few bytes: an error, nothing read past the end.
            let mut short = vec![zero; vs.len() + 1];
            assert!(T::unpack_all(&one, &mut short).is_err());
        }
        let k = 0..1000u32;
        check(&k.clone().map(|i| (i * 37) as u8).collect::<Vec<_>>(), 0);
        check(&k.clone().map(|i| (i * 37) as i8).collect::<Vec<_>>(), 0);
        check(&k.clone().map(|i| (i * 7919) as u16).collect::<Vec<_>>(), 0);
        check(&k.clone().map(|i| (i * 7919) as i16).collect::<Vec<_>>(), 0);
        check(
            &k.clone()
                .map(|i| i.wrapping_mul(2654435761))
                .collect::<Vec<_>>(),
            0,
        );
        check(
            &k.clone()
                .map(|i| i.wrapping_mul(2654435761) as i32)
                .collect::<Vec<_>>(),
            0,
        );
        check(
            &k.clone()
                .map(|i| u64::from(i) << 40 | 5)
                .collect::<Vec<_>>(),
            0,
        );
        check(
            &k.clone().map(|i| -(i64::from(i) << 33)).collect::<Vec<_>>(),
            0,
        );
        check(
            &k.clone()
                .map(|i| f32::from_bits(i.wrapping_mul(2654435761)))
                .filter(|x| !x.is_nan())
                .collect::<Vec<_>>(),
            0.0,
        );
        let f: Vec<f64> = k.map(|i| f64::from(i) * -1.5e-300 + 0.1).collect();
        check(&f, 0.0);
        // Bit patterns survive (NaN payloads, signed zero).
        let odd = [f64::from_bits(0x7ff8_dead_beef_0001), -0.0, f64::INFINITY];
        let mut b = Vec::new();
        f64::pack_all(&odd, &mut b);
        let mut back = [0.0; 3];
        f64::unpack_all(&b, &mut back).unwrap();
        assert!(
            odd.iter()
                .zip(&back)
                .all(|(a, c)| a.to_bits() == c.to_bits())
        );
        // Buffer level: the read position advances only on success.
        let mut buf = CommunicationBuffer::new();
        buf.pack_slice(&[1u32, 2, 3]);
        let mut four = [0u32; 4];
        assert!(buf.unpack_slice(&mut four).is_err());
        let mut three = [0u32; 3];
        buf.unpack_slice(&mut three).unwrap();
        assert_eq!(three, [1, 2, 3]);
    }

    #[test]
    fn size_of_vec_saturates() {
        assert_eq!(CommunicationBuffer::size_of_vec::<f64>(3), 8 + 24);
        assert_eq!(
            CommunicationBuffer::size_of_vec::<f64>(usize::MAX / 4),
            usize::MAX
        );
        assert_eq!(
            CommunicationBuffer::size_of_vec::<u8>(usize::MAX),
            usize::MAX
        );
    }

    #[test]
    fn roundtrip() {
        let mut b = CommunicationBuffer::new();
        let e = Element::new(ElementType::Hexahedron20, 123_456, GhostType::Ghost);
        b.pack(&1.5f64);
        b.pack(&-3i32);
        b.pack(&e);
        b.pack(&true);
        b.pack(&NodeFlag::SLAVE);
        b.pack_slice(&[1u64, 2, 3]);
        assert_eq!(b.len(), 8 + 4 + 6 + 1 + 1 + 24);
        assert_eq!(b.unpack::<f64>().unwrap(), 1.5);
        assert_eq!(b.unpack::<i32>().unwrap(), -3);
        assert_eq!(b.unpack::<Element>().unwrap(), e);
        assert!(b.unpack::<bool>().unwrap());
        assert_eq!(b.unpack::<NodeFlag>().unwrap(), NodeFlag::SLAVE);
        let mut out = [0u64; 3];
        b.unpack_slice(&mut out).unwrap();
        assert_eq!(out, [1, 2, 3]);
        assert_eq!(b.remaining(), 0);
        assert!(b.unpack::<u8>().is_err());
        b.rewind();
        assert_eq!(b.unpack::<f64>().unwrap(), 1.5);
    }

    #[test]
    fn length_prefixed_sequences_and_strings() {
        let mut b = CommunicationBuffer::new();
        b.pack_vec(&[1i32, -2, 3]);
        b.pack_str("ghost élément");
        assert_eq!(
            b.len(),
            CommunicationBuffer::size_of_vec::<i32>(3)
                + CommunicationBuffer::size_of_vec::<u8>("ghost élément".len())
        );
        assert_eq!(b.unpack_vec::<i32>().unwrap(), vec![1, -2, 3]);
        assert_eq!(b.unpack_string().unwrap(), "ghost élément");
        assert_eq!(b.remaining(), 0);
        // A corrupt count never allocates beyond the buffer.
        let mut bad = CommunicationBuffer::new();
        bad.pack(&u64::MAX);
        assert!(bad.unpack_vec::<f64>().is_err());
    }
}
