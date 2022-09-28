use std::cell::RefMut;
use std::ops::{Deref, DerefMut};

use super::wakers::WakeGuard;

/// RAII structure used to release the exclusive write access of a lock when dropped.
///
/// This structure is created by the `write` method on [`RwLock`](super::RwLock).
#[derive(Debug)]
pub struct RwLockWriteGuard<'a, T: ?Sized> {
    pub(super) wake_guard: WakeGuard<'a>,
    // RefMut has a must_not_suspend notation. However, this lint is unlikely to be enabled in the
    // foreseeable future. When it becomes enabled, we can vendor a version of RefCell without the
    // lint.
    pub(super) val: RefMut<'a, T>,
}

impl<'a, T> RwLockWriteGuard<'a, T>
where
    T: ?Sized,
{
    /// Makes a new `RwLockWriteGuard` for a component of the locked data.
    ///
    /// This operation cannot fail as data is already locked.
    ///
    /// This is an associated function that needs to be used as `RwLockWriteGuard::map(...)`. A
    /// method would interfere with methods of the same name on the contents of the underlying data
    /// used through `Deref`.
    pub fn map<U, F>(this: Self, f: F) -> RwLockWriteGuard<'a, U>
    where
        F: FnOnce(&mut T) -> &mut U,
        U: ?Sized,
    {
        let Self { wake_guard, val } = this;

        let val = RefMut::map(val, f);
        RwLockWriteGuard { wake_guard, val }
    }

    /// Tries to make a new `RwLockWriteGuard` for a component of the locked data. Returns the
    /// original write guard if the closure returns `None`.
    ///
    /// This operation cannot fail as data is already locked.
    ///
    /// This is an associated function that needs to be used as `RwLockWriteGuard::filter_map(...)`.
    /// A method would interfere with methods of the same name on the contents of the underlying
    /// data used through `Deref`.
    ///
    /// This function is not available before Rust 1.63.
    #[rustversion::since(1.63)]
    pub fn filter_map<U, F>(this: Self, f: F) -> Result<RwLockWriteGuard<'a, U>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
        U: ?Sized,
    {
        let Self { wake_guard, val } = this;

        match RefMut::filter_map(val, f) {
            Ok(val) => Ok(RwLockWriteGuard { wake_guard, val }),
            Err(val) => Err(RwLockWriteGuard { wake_guard, val }),
        }
    }
}

impl<T> Deref for RwLockWriteGuard<'_, T>
where
    T: ?Sized,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.val.deref()
    }
}

impl<T> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut <Self as Deref>::Target {
        self.val.deref_mut()
    }
}
