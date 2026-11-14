// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/synchronizer/data_accessor.hh,
// data_accessor_tmpl.hh (packUnpackDOFDataHelper, ReduceDataAccessor,
// SimpleIdxDataAccessor).

//! Generic data accessors (MPI-009).
//!
//! Upstream offers, besides the abstract `DataAccessor<T>`, helpers that
//! pack/unpack `nb_component` values per entity of a flat array
//! (`packDOFDataHelper`), an identity accessor on `Idx` entities
//! (`SimpleIdxDataAccessor`, used by `synchronizeArray`) and a reducing one
//! (`ReduceDataAccessor<Idx, Op, T>`, used by `reduceSynchronizeArray`).
//! The element helpers (`packNodalDataHelper`, `packElementalDataHelper`)
//! need the mesh / FE engine and belong to the mesh-aware synchronizers.
//!
//! Entities index a flat array of `nb_component`-sized records; an index
//! outside the array is an error (never a panic: indices may come from a
//! peer's scheme). Every record end and size is computed with checked
//! arithmetic; `nb_data` saturates at `usize::MAX`,
//! which the synchronizer's preparation rejects (agreed on every rank)
//! before anything is allocated.

use akantu_core::enums::SynchronizationTag;
use akantu_core::{Error, Idx, Result};

use crate::buffer::{CommunicationBuffer, Packable};
use crate::communicator::{CommData, ReduceOp, reduce_value};
use crate::synchronizer::DataAccessor;

fn record(entity: Idx, nb_component: usize, len: usize) -> Result<std::ops::Range<usize>> {
    let i = usize::try_from(entity)
        .map_err(|_| Error::InvalidArgument(format!("negative entity index {entity}")))?;
    let end = i
        .checked_mul(nb_component)
        .and_then(|start| start.checked_add(nb_component).map(|end| (start, end)));
    let Some((start, end)) = end else {
        return Err(Error::InvalidArgument(format!(
            "entity {entity} × {nb_component} components overflows"
        )));
    };
    if end > len {
        return Err(Error::InvalidArgument(format!(
            "entity {entity} out of range ({} records of {nb_component} components)",
            len / nb_component.max(1)
        )));
    }
    Ok(start..end)
}

/// Packed size of `n` records of `nb_component` values, saturating at
/// `usize::MAX` on overflow (rejected by the synchronizer's preparation).
#[must_use]
pub fn record_bytes<T: Packable>(n: usize, nb_component: usize) -> usize {
    n.checked_mul(nb_component)
        .and_then(|v| v.checked_mul(T::SIZE))
        .unwrap_or(usize::MAX)
}

/// Pack the `nb_component` values of each entity of `data` (upstream
/// `packDOFDataHelper`).
pub fn pack_entity_data<T: Packable + Copy>(
    data: &[T],
    nb_component: usize,
    buffer: &mut CommunicationBuffer,
    entities: &[Idx],
) -> Result<()> {
    for &e in entities {
        buffer.pack_slice(&data[record(e, nb_component, data.len())?]);
    }
    Ok(())
}

/// Unpack the `nb_component` values of each entity into `data` (upstream
/// `unpackDOFDataHelper`).
pub fn unpack_entity_data<T: Packable + Copy>(
    data: &mut [T],
    nb_component: usize,
    buffer: &mut CommunicationBuffer,
    entities: &[Idx],
) -> Result<()> {
    for &e in entities {
        let r = record(e, nb_component, data.len())?;
        buffer.unpack_slice(&mut data[r])?;
    }
    Ok(())
}

/// Identity accessor on a flat array (upstream `SimpleIdxDataAccessor<T>`):
/// unpacking overwrites the received entities' records. Only `tag`
/// communicates (other tags have no data).
#[derive(Debug)]
pub struct SimpleIdxDataAccessor<'a, T> {
    data: &'a mut [T],
    nb_component: usize,
    tag: SynchronizationTag,
}

impl<'a, T: CommData> SimpleIdxDataAccessor<'a, T> {
    /// Accessor over `data` (`nb_component` values per entity) for `tag`.
    pub fn new(data: &'a mut [T], nb_component: usize, tag: SynchronizationTag) -> Self {
        Self {
            data,
            nb_component,
            tag,
        }
    }
}

impl<T: CommData> DataAccessor<Idx> for SimpleIdxDataAccessor<'_, T> {
    fn nb_data(&self, entities: &[Idx], tag: SynchronizationTag) -> usize {
        if tag == self.tag {
            record_bytes::<T>(entities.len(), self.nb_component)
        } else {
            0
        }
    }
    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        entities: &[Idx],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != self.tag {
            return Ok(());
        }
        pack_entity_data(self.data, self.nb_component, buffer, entities)
    }
    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        entities: &[Idx],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != self.tag {
            return Ok(());
        }
        unpack_entity_data(self.data, self.nb_component, buffer, entities)
    }
}

/// Reducing accessor on a flat array (upstream `ReduceDataAccessor<Idx,
/// Op, T>`): unpacking combines `value = op(value, received)` component by
/// component (`AddOperation` = [`ReduceOp::Sum`], …). Only `tag`
/// communicates.
#[derive(Debug)]
pub struct ReduceDataAccessor<'a, T> {
    data: &'a mut [T],
    nb_component: usize,
    tag: SynchronizationTag,
    op: ReduceOp,
}

impl<'a, T: CommData> ReduceDataAccessor<'a, T> {
    /// Accessor over `data` for `tag`, combining received values with `op`.
    pub fn new(
        data: &'a mut [T],
        nb_component: usize,
        tag: SynchronizationTag,
        op: ReduceOp,
    ) -> Self {
        Self {
            data,
            nb_component,
            tag,
            op,
        }
    }
}

impl<T: CommData> DataAccessor<Idx> for ReduceDataAccessor<'_, T> {
    fn nb_data(&self, entities: &[Idx], tag: SynchronizationTag) -> usize {
        if tag == self.tag {
            record_bytes::<T>(entities.len(), self.nb_component)
        } else {
            0
        }
    }
    fn pack_data(
        &self,
        buffer: &mut CommunicationBuffer,
        entities: &[Idx],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != self.tag {
            return Ok(());
        }
        pack_entity_data(self.data, self.nb_component, buffer, entities)
    }
    fn unpack_data(
        &mut self,
        buffer: &mut CommunicationBuffer,
        entities: &[Idx],
        tag: SynchronizationTag,
    ) -> Result<()> {
        if tag != self.tag {
            return Ok(());
        }
        for &e in entities {
            let r = record(e, self.nb_component, self.data.len())?;
            for v in &mut self.data[r] {
                let received: T = buffer.unpack()?;
                *v = reduce_value(*v, received, self.op)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record end that overflows
    /// is an error, never a panic, and allocates nothing (debug and
    /// release).
    #[test]
    fn record_end_overflow_is_an_error() {
        let mut b = CommunicationBuffer::new();
        let r = pack_entity_data::<u8>(&[], usize::MAX, &mut b, &[1]);
        assert!(r.is_err(), "{r:?}");
        let r = pack_entity_data::<u8>(&[], usize::MAX, &mut b, &[0]);
        assert!(r.is_err(), "start 0, end usize::MAX is out of range: {r:?}");
        let mut out: [u8; 0] = [];
        let mut src = CommunicationBuffer::from_bytes(vec![0; 8]);
        assert!(unpack_entity_data(&mut out, usize::MAX / 2 + 1, &mut src, &[1]).is_err());
        assert!(b.is_empty());
    }

    #[test]
    fn helpers_roundtrip_and_reject_bad_indices() {
        let data = [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0];
        let mut b = CommunicationBuffer::new();
        pack_entity_data(&data, 2, &mut b, &[2, 0]).unwrap();
        assert_eq!(b.len(), 4 * 8);
        let mut out = [0.0f64; 6];
        unpack_entity_data(&mut out, 2, &mut b, &[0, 1]).unwrap();
        assert_eq!(out, [5.0, 6.0, 1.0, 2.0, 0.0, 0.0]);
        assert!(pack_entity_data(&data, 2, &mut b, &[3]).is_err());
        assert!(pack_entity_data(&data, 2, &mut b, &[-1]).is_err());
        assert_eq!(record_bytes::<f64>(usize::MAX / 2, 3), usize::MAX);

        let mut acc_data = [1i64, 2, 3];
        let mut acc = ReduceDataAccessor::new(
            &mut acc_data,
            1,
            SynchronizationTag::Whatever,
            ReduceOp::Sum,
        );
        assert_eq!(
            acc.nb_data(&[0, 2], SynchronizationTag::SmmUv),
            0,
            "other tags carry nothing"
        );
        let mut b = CommunicationBuffer::new();
        b.pack_slice(&[10i64, 20]);
        acc.unpack_data(&mut b, &[0, 2], SynchronizationTag::Whatever)
            .unwrap();
        assert_eq!(acc_data, [11, 2, 23]);
    }
}
