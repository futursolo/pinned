use std::cell::RefCell;
use std::task::{Context, Poll};

use futures::future::poll_fn;

mod error;
mod read_guard;
mod wakers;
mod write_guard;
pub use error::*;
pub use read_guard::*;
use wakers::Wakers;
pub use write_guard::*;

/// An asynchronous reader-writer lock.
#[derive(Debug)]
pub struct RwLock<T: ?Sized> {
    wakers: Wakers,
    val: RefCell<T>,
}

impl<T> RwLock<T> {
    /// Creates a new `RwLock` containing value `T`
    pub fn new(val: T) -> Self {
        Self {
            wakers: Wakers::new(),
            val: RefCell::new(val),
        }
    }

    /// Consumes the lock, returning the underlying data.
    pub fn into_inner(self) -> T {
        self.val.into_inner()
    }
}

impl<T> RwLock<T>
where
    T: ?Sized,
{
    /// Attempts to acquire this `RwLock` with shared read access.
    ///
    /// If the access couldn’t be acquired immediately, returns [`TryLockError`]. Otherwise, an RAII
    /// guard is returned which will release read access when dropped.
    ///
    /// This function does not block.
    ///
    /// This function does not provide any guarantees with respect to the ordering of whether
    /// contentious readers or writers will acquire the lock first.
    pub fn try_read(&self) -> TryLockResult<RwLockReadGuard<'_, T>> {
        let read_inner = self.val.try_borrow().map_err(|_| TryLockError::new())?;
        let wake_guard = self.wakers.wake_guard();

        Ok(RwLockReadGuard {
            val: read_inner,
            wake_guard,
        })
    }

    /// Attempts to lock this `RwLock` with exclusive write access.
    ///
    /// If the lock could not be acquired immediately, returns [`TryLockError`]. Otherwise, an RAII
    /// guard is returned which will release the lock when it is dropped.
    ///
    /// This function does not block.
    ///
    /// This function does not provide any guarantees with respect to the ordering of whether
    /// contentious readers or writers will acquire the lock first.
    pub fn try_write(&self) -> TryLockResult<RwLockWriteGuard<'_, T>> {
        let write_inner = self.val.try_borrow_mut().map_err(|_| TryLockError::new())?;
        let wake_guard = self.wakers.wake_guard();

        Ok(RwLockWriteGuard {
            val: write_inner,
            wake_guard,
        })
    }

    fn poll_read(&self, cx: &mut Context<'_>) -> Poll<RwLockReadGuard<'_, T>> {
        if let Ok(m) = self.try_read() {
            return Poll::Ready(m);
        }

        self.wakers.push(cx.waker().clone());

        Poll::Pending
    }

    /// Locks the current `RwLock` with shared read access, causing the current task to yield
    /// until the lock has been acquired.
    ///
    /// This method does not provide any guarantees with respect to the ordering of whether
    /// contentious readers or writers will acquire the lock first.
    ///
    /// Returns an RAII guard which will release this task's shared access once it is dropped.
    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        poll_fn(|cx| self.poll_read(cx)).await
    }

    fn poll_write(&self, cx: &mut Context<'_>) -> Poll<RwLockWriteGuard<'_, T>> {
        if let Ok(m) = self.try_write() {
            return Poll::Ready(m);
        }

        self.wakers.push(cx.waker().clone());

        Poll::Pending
    }

    /// Locks the current `RwLock` with exclusive write access, causing the current task to yield
    /// until the lock has been acquired.
    ///
    /// This method does not provide any guarantees with respect to the ordering of whether
    /// contentious readers or writers will acquire the lock first.
    ///
    /// Returns an RAII guard which will drop the write access once it is dropped.
    pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
        poll_fn(|cx| self.poll_write(cx)).await
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// This call borrows `RwLock` mutably (at compile-time) so there is no need for dynamic checks.
    pub fn get_mut(&mut self) -> &mut T {
        self.val.get_mut()
    }
}
