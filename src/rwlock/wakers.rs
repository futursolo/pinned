//! A Waker collection.
//!
//! We need to wake all pending tasks as the task is not guaranteed that they are being actively
//! polled / already cancelled.

use std::task::Waker;

use crate::cell::UnsafeCell;

#[derive(Debug, Clone)]
pub(super) struct WakeGuard<'a> {
    wakers: &'a Wakers,
}

impl Drop for WakeGuard<'_> {
    fn drop(&mut self) {
        self.wakers.wake_all();
    }
}

#[derive(Debug)]
pub(super) struct Wakers {
    inner: UnsafeCell<Vec<Waker>>,
}

impl Wakers {
    pub fn new() -> Self {
        Self {
            inner: UnsafeCell::new(Vec::new()),
        }
    }

    pub fn push(&self, w: Waker) {
        // SAFETY:
        //
        // We can acquire a mutable reference without checking as:
        //
        // - This type is !Sync and !Send.
        // - This function is not used by any other functions and hence uniquely owns the
        // mutable reference.
        // - The mutable reference is dropped at the end of this function.
        unsafe {
            self.inner.with_mut(move |inner| inner.push(w));
        }
    }

    pub fn wake_guard(&self) -> WakeGuard<'_> {
        WakeGuard { wakers: self }
    }

    pub fn wake_all(&self) {
        // SAFETY:
        //
        // We can acquire a mutable reference without checking as:
        //
        // - This type is !Sync and !Send.
        // - This function is not used by any other functions and hence uniquely owns the
        // mutable reference.
        // - The mutable reference is dropped at the end of this function.
        unsafe {
            self.inner.with_mut(|inner| {
                for waker in inner.drain(..) {
                    waker.wake()
                }
            });
        }
    }
}
