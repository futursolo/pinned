use std::marker::PhantomData;

/// A scoped variant of UnsafeCell.
///
/// Inspired by tokio::loom::cell::UnsafeCell.
///
/// This type provides additional protections over std::cell::UnsafeCell:
///
/// - Using await statements without releasing the reference.
/// - Attempting to return the reference.
#[derive(Debug)]
pub(crate) struct UnsafeCell<T> {
    // This type is not Send or Sync.
    _marker: PhantomData<*const ()>,

    inner: std::cell::UnsafeCell<T>,
}

impl<T> UnsafeCell<T> {
    #[inline]
    pub(crate) const fn new(data: T) -> UnsafeCell<T> {
        UnsafeCell {
            inner: std::cell::UnsafeCell::new(data),
            _marker: PhantomData,
        }
    }

    // SAFETY:
    //
    // This function cannot be called recursively.
    #[inline]
    pub(crate) unsafe fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&*self.inner.get())
    }

    // SAFETY:
    //
    // This function cannot be called recursively.
    #[inline]
    pub(crate) unsafe fn with_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        f(&mut *self.inner.get())
    }
}
