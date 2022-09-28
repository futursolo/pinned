use std::marker::PhantomData;
use std::result;

use thiserror::Error;

/// Error returned from the [`RwLock::try_read`](super::RwLock::try_read) and
/// [`RwLock::try_write`](super::RwLock::try_write)
/// functions.
///
/// `RwLock::try_read` operation will only fail if the lock is currently held by an exclusive
/// writer.
///
/// `RwLock::try_write` operation will fail if the lock is held by any reader or by an exclusive
/// writer.
#[derive(Debug, Error)]
#[error("this operation would block.")]
pub struct TryLockError(PhantomData<()>);

impl TryLockError {
    pub(super) fn new() -> Self {
        Self(PhantomData)
    }
}

/// A type alias for the result of a nonblocking locking method.
pub type TryLockResult<T> = result::Result<T, TryLockError>;
