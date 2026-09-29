use alloc::vec::Vec;
use core::{cell::UnsafeCell, ops::Range, ptr};

cfg_if::cfg_if! {
    if #[cfg(supports_ptr_atomics)] {
        use alloc::sync::Arc;
    } else if #[cfg(feature = "portable-atomic")] {
        use portable_atomic_util::Arc;
    }
}

#[derive(Clone, Debug)]
pub struct Buffer {

    storage: Arc<UnsafeCell<[u8]>>,

    size: usize,
}

unsafe impl Send for Buffer {}
unsafe impl Sync for Buffer {}

impl Buffer {
    pub(super) fn new(desc: &crate::BufferDescriptor) -> Result<Self, crate::DeviceError> {
        let &crate::BufferDescriptor {
            label: _,
            size,
            usage: _,
            memory_flags: _,
        } = desc;

        let size = usize::try_from(size).map_err(|_| crate::DeviceError::OutOfMemory)?;

        let mut vector: Vec<u8> = Vec::new();
        vector
            .try_reserve_exact(size)
            .map_err(|_| crate::DeviceError::OutOfMemory)?;
        vector.resize(size, 0);
        let storage: Arc<[u8]> = Arc::from(vector);
        debug_assert_eq!(storage.len(), size);

        let storage: Arc<UnsafeCell<[u8]>> =
            unsafe { Arc::from_raw(Arc::into_raw(storage) as *mut UnsafeCell<[u8]>) };

        Ok(Buffer { storage, size })
    }

    pub(super) fn get_slice_ptr(&self, range: crate::MemoryRange) -> *mut [u8] {
        let base_ptr = self.storage.get();
        let range = range_to_usize(range, self.size);

        ptr::slice_from_raw_parts_mut(

            unsafe { base_ptr.cast::<u8>().add(range.start) },
            range.len(),
        )
    }
}

fn range_to_usize(range: crate::MemoryRange, upper_bound: usize) -> Range<usize> {

    let start = usize::try_from(range.start).expect("range too large");
    let end = usize::try_from(range.end).expect("range too large");
    assert!(start <= end && end <= upper_bound, "range out of bounds");
    start..end
}
