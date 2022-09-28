use std::cell::{Cell, RefCell};
use std::task::Poll;

use futures::future::poll_fn;

use crate::utils::yield_now;

mod error;
mod read_guard;
mod wakers;
mod write_guard;
pub use error::*;
pub use read_guard::*;
use wakers::Wakers;
pub use write_guard::*;

/// An asynchronous reader-writer lock.
///
/// This type of lock allows a number of readers or at most one writer at any point in time. The
/// write portion of this lock typically allows modification of the underlying data (exclusive
/// access) and the read portion of this lock typically allows for read-only access (shared access).
///
/// The acquisition order of this lock is not guaranteed and depending on the runtime's
/// implementation and preference of any used polling combinators.
///
/// # Examples
///
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// # use pinned::RwLock;
/// let lock = RwLock::new(5);
///
/// // many reader locks can be held at once
/// {
///     let r1 = lock.read().await;
///     let r2 = lock.read().await;
///     assert_eq!(*r1, 5);
///     assert_eq!(*r2, 5);
/// } // read locks are dropped at this point
///
/// // only one write lock may be held, however
/// {
///     let mut w = lock.write().await;
///     *w += 1;
///     assert_eq!(*w, 6);
/// } // write lock is dropped here
/// # }
/// ```
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

    /// Wait for the next lock release.
    async fn wait(&self) {
        let awaited = Cell::new(false);

        poll_fn(move |cx| {
            if awaited.get() {
                return Poll::Ready(());
            }

            awaited.set(true);

            self.wakers.push(cx.waker().clone());
            Poll::Pending
        })
        .await;
    }

    /// Locks the current `RwLock` with shared read access, causing the current task to yield
    /// until the lock has been acquired.
    ///
    /// This method does not provide any guarantees with respect to the ordering of whether
    /// contentious readers or writers will acquire the lock first.
    ///
    /// Returns an RAII guard which will release this task's shared access once it is dropped.
    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        // We yield to provide some fairness over the current runtime / polling combinator so that
        // one task is not starving the lock.
        yield_now().await;

        loop {
            if let Ok(m) = self.try_read() {
                return m;
            }
            self.wait().await;
        }
    }

    /// Locks the current `RwLock` with exclusive write access, causing the current task to yield
    /// until the lock has been acquired.
    ///
    /// This method does not provide any guarantees with respect to the ordering of whether
    /// contentious readers or writers will acquire the lock first.
    ///
    /// Returns an RAII guard which will drop the write access once it is dropped.
    pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
        // We yield to provide some fairness over the current runtime / polling combinator so that
        // one task is not starving the lock.
        yield_now().await;

        loop {
            if let Ok(m) = self.try_write() {
                return m;
            }
            self.wait().await;
        }
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// This call borrows `RwLock` mutably (at compile-time) so there is no need for dynamic checks.
    pub fn get_mut(&mut self) -> &mut T {
        self.val.get_mut()
    }
}

#[cfg(test)]
mod tests {
    //! tests, mostly borrowed from tokio, adapted to this implementation.

    use std::rc::Rc;
    use std::time::Duration;

    use futures::future::FutureExt;
    use futures::{pin_mut, poll};
    use tokio::test;
    use tokio::time::timeout;

    use super::*;

    static SEC_5: Duration = Duration::from_secs(5);

    #[test]
    async fn into_inner() {
        let rwlock = RwLock::new(42);
        assert_eq!(rwlock.into_inner(), 42);
    }

    #[test]
    async fn read_shared() {
        timeout(SEC_5, async {
            let rwlock = RwLock::new(100);

            let _r1 = rwlock.read().await;
            let _r2 = rwlock.read().await;
        })
        .await
        .expect("timed out")
    }

    #[test]
    async fn write_shared_pending() {
        timeout(SEC_5, async {
            let rwlock = RwLock::new(100);

            let _r1 = rwlock.read().await;
            timeout(Duration::from_millis(500), rwlock.write())
                .await
                .expect_err("not timed out?");
        })
        .await
        .expect("timed out");
    }

    #[test]
    async fn read_exclusive_pending() {
        timeout(SEC_5, async {
            let rwlock = RwLock::new(100);

            let _w1 = rwlock.write().await;
            timeout(Duration::from_millis(500), rwlock.read())
                .await
                .expect_err("not timed out?");
        })
        .await
        .expect("timed out");
    }

    #[test]
    async fn write_exclusive_pending() {
        timeout(SEC_5, async {
            let rwlock = RwLock::new(100);

            let _w1 = rwlock.write().await;
            timeout(Duration::from_millis(500), rwlock.write())
                .await
                .expect_err("not timed out?");
        })
        .await
        .expect("timed out");
    }

    #[test]
    async fn write_shared_drop() {
        timeout(SEC_5, async {
            let rwlock = Rc::new(RwLock::new(100));

            let rwlock = rwlock.clone();
            let w1 = rwlock.write().await;

            let try_write_2 = rwlock.write();
            pin_mut!(try_write_2);

            matches!(poll!(&mut try_write_2), Poll::Pending);
            matches!(poll!(&mut try_write_2), Poll::Pending);
            matches!(poll!(&mut try_write_2), Poll::Pending);

            drop(w1);

            try_write_2.await;
        })
        .await
        .expect("timed out");
    }

    #[test]
    async fn write_pending_read_shared_ready() {
        timeout(SEC_5, async {
            let rwlock = RwLock::new(100);

            let _r1 = rwlock.read().await;
            let _r2 = rwlock.read().await;

            let try_write_1 = rwlock.write();
            pin_mut!(try_write_1);

            matches!(poll!(&mut try_write_1), Poll::Pending);
            matches!(poll!(&mut try_write_1), Poll::Pending);
            matches!(poll!(&mut try_write_1), Poll::Pending);
            let _r3 = rwlock.read().await;

            timeout(Duration::from_millis(500), try_write_1)
                .await
                .expect_err("not timed out?");
        })
        .await
        .expect("timed out");
    }

    #[test]
    async fn read_uncontested() {
        let rwlock = RwLock::new(100);
        let result = *rwlock.read().await;

        assert_eq!(result, 100);
    }

    #[test]
    async fn write_uncontested() {
        let rwlock = RwLock::new(100);
        let mut result = rwlock.write().await;
        *result += 50;
        assert_eq!(*result, 150);
    }

    #[test]
    async fn write_order() {
        let rwlock = RwLock::<Vec<u32>>::new(vec![]);
        let fut2 = rwlock.write().map(|mut guard| guard.push(2));
        let fut1 = rwlock.write().map(|mut guard| guard.push(1));
        fut1.await;
        fut2.await;

        let g = rwlock.read().await;
        assert_eq!(*g, vec![1, 2]);
    }

    #[test]
    async fn try_write() {
        let lock = RwLock::new(0);
        let read_guard = lock.read().await;
        assert!(lock.try_write().is_err());
        drop(read_guard);
        assert!(lock.try_write().is_ok());
    }

    #[test]
    async fn try_read_try_write() {
        let lock: RwLock<usize> = RwLock::new(15);

        {
            let rg1 = lock.try_read().unwrap();
            assert_eq!(*rg1, 15);

            assert!(lock.try_write().is_err());

            let rg2 = lock.try_read().unwrap();
            assert_eq!(*rg2, 15)
        }

        {
            let mut wg = lock.try_write().unwrap();
            *wg = 1515;

            assert!(lock.try_read().is_err())
        }

        assert_eq!(*lock.try_read().unwrap(), 1515);
    }
}
