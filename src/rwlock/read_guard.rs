use std::cell::Ref;
use std::ops::Deref;

use super::wakers::WakeGuard;

/// RAII structure used to release the shared read access of a lock when dropped.
///
/// This structure is created by the `read` method on [`RwLock`](super::RwLock).
#[derive(Debug)]
pub struct RwLockReadGuard<'a, T: ?Sized> {
    pub(super) wake_guard: WakeGuard<'a>,
    // Ref has a must_not_suspend notation. However, this lint is unlikely to be enabled in the
    // foreseeable future. When it becomes enabled, we can vendor a version of RefCell without the
    // lint.
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
        let Self { wake_guard, val } = this;

        let val = Ref::map(val, f);
        RwLockReadGuard { wake_guard, val }
    }

    /// Tries to make a new `RwLockReadGuard` for a component of the locked data. Returns the
    /// original read guard if the closure returns `None`.
    ///
    /// This operation cannot fail as data is already locked.
    ///
    /// This is an associated function that needs to be used as `RwLockReadGuard::filter_map(...)`.
    /// A method would interfere with methods of the same name on the contents of the underlying
    /// data used through `Deref`.
    ///
    /// This function is not available before Rust 1.63.
    #[rustversion::since(1.63)]
    pub fn filter_map<U, F>(this: Self, f: F) -> Result<RwLockReadGuard<'a, U>, Self>
    where
        F: FnOnce(&T) -> Option<&U>,
        U: ?Sized,
    {
        let Self { wake_guard, val } = this;

        match Ref::filter_map(val, f) {
            Ok(val) => Ok(RwLockReadGuard { wake_guard, val }),
            Err(val) => Err(RwLockReadGuard { wake_guard, val }),
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
