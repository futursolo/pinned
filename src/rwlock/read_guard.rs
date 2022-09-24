use std::cell::Ref;
use std::ops::Deref;

use super::wakers::WakeGuard;

/// RAII structure used to release the shared read access of a lock when dropped.
///
/// This structure is created by the [`read`] method on [`RwLock`].
#[derive(Debug)]
pub struct RwLockReadGuard<'a, T: ?Sized> {
    pub(super) wake_guard: WakeGuard<'a>,
    pub(super) val: Ref<'a, T>,
}

impl<'a, T> RwLockReadGuard<'a, T>
where
    T: ?Sized,
{
    /// Makes a new `RwLockReadGuard` for a component of the locked data.
    ///
    /// This operation cannot fail as data is already locked.
    ///
    /// This is an associated function that needs to be used as `RwLockReadGuard::map(...)`. A
    /// method would interfere with methods of the same name on the contents of the underlying data
    /// used through `Deref`.
    pub fn map<U, F>(this: Self, f: F) -> RwLockReadGuard<'a, U>
    where
        F: FnOnce(&T) -> &U,
        U: ?Sized,
    {
        let read_inner = Ref::map(this.val, f);

        RwLockReadGuard {
            wake_guard: this.wake_guard,
            val: read_inner,
        }
    }

    /// Tries to make a new `RwLockReadGuard` for a component of the locked data. Returns the
    /// original read guard if the closure returns `None`.
    ///
    /// This operation cannot fail as data is already locked.
    ///
    /// This is an associated function that needs to be used as `RwLockReadGuard::filter_map(...)`.
    /// A method would interfere with methods of the same name on the contents of the underlying
    /// data used through `Deref`.
    pub fn filter_map<U, F>(
        this: RwLockReadGuard<'a, T>,
        f: F,
    ) -> Result<RwLockReadGuard<'a, U>, Self>
    where
        F: FnOnce(&T) -> Option<&U>,
        U: ?Sized,
    {
        match Ref::filter_map(this.val, f) {
            Ok(val) => Ok(RwLockReadGuard {
                wake_guard: this.wake_guard,
                val,
            }),
            Err(val) => Err(RwLockReadGuard {
                wake_guard: this.wake_guard,
                val,
            }),
        }
    }
}

impl<T> Deref for RwLockReadGuard<'_, T>
where
    T: ?Sized,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.val.deref()
    }
}