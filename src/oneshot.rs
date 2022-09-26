//! A channel that can pass one single value from a sender to a receiver.

use std::future::Future;
use std::marker::PhantomData;
use std::rc::Rc;
use std::task::{Poll, Waker};

use thiserror::Error;

use crate::cell::UnsafeCell;

/// Error returned when the channel is closed before a value is sent to the [`Receiver`].
#[derive(Debug, Error)]
#[error("channel has been closed.")]
pub struct RecvError {
    _marker: PhantomData<()>,
}

#[derive(Debug)]
struct Inner<T> {
    rx_waker: Option<Waker>,
    closed: bool,
    item: Option<T>,

    // This type is not send or sync.
    _marker: PhantomData<Rc<()>>,
}

impl<T> Inner<T> {
    #[inline]
    fn poll_impl(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Result<T, RecvError>> {
        // Implementation Note:
        //
        // It might be nicer to use a match pattern here.
        // However, this will slow down the polling process by 10%.
        if let Some(m) = self.item.take() {
            return Poll::Ready(Ok(m));
        }

        if self.closed {
            return Poll::Ready(Err(RecvError {
                _marker: PhantomData,
            }));
        }

        self.rx_waker = Some(cx.waker().clone());
        Poll::Pending
    }

    #[inline]
    fn send_impl(&mut self, item: T) -> Result<(), T> {
        if self.closed {
            return Err(item);
        }

        self.item = Some(item);

        if let Some(ref m) = self.rx_waker {
            m.wake_by_ref();
        }

        Ok(())
    }

    fn close_impl(&mut self, wake_recv: bool) {
        self.closed = true;

        if wake_recv && self.item.is_none() {
            if let Some(ref m) = self.rx_waker {
                m.wake_by_ref();
            }
        }
    }
}

/// The receiver of a oneshot channel.
///
/// This type has no recv method. To receive the value, `await` the receiver.
#[derive(Debug)]
pub struct Receiver<T> {
    inner: Rc<UnsafeCell<Inner<T>>>,
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, RecvError>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        // SAFETY:
        //
        // We can acquire a mutable reference without checking as:
        //
        // - This type is !Sync and !Send.
        // - This function is not used by any other functions and hence uniquely owns the
        // mutable reference.
        // - The mutable reference is dropped at the end of this function.
        unsafe { self.inner.with_mut(|inner| inner.poll_impl(cx)) }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        // SAFETY:
        //
        // We can acquire a mutable reference without checking as:
        //
        // - This type is !Sync and !Send.
        // - This function is not used by any other functions and hence uniquely owns the
        // mutable reference.
        // - The mutable reference is dropped at the end of this function.
        unsafe { self.inner.with_mut(|inner| inner.close_impl(false)) }
    }
}

/// The sender of a oneshot channel.
#[derive(Debug)]
pub struct Sender<T> {
    inner: Rc<UnsafeCell<Inner<T>>>,
}

impl<T> Sender<T> {
    /// Send an item to the other side of the channel, consumes the sender.
    pub fn send(self, item: T) -> Result<(), T> {
        // SAFETY:
        //
        // We can acquire a mutable reference without checking as:
        //
        // - This type is !Sync and !Send.
        // - This function is not used by any other functions and hence uniquely owns the
        // mutable reference.
        // - The mutable reference is dropped at the end of this function.
        unsafe { self.inner.with_mut(move |inner| inner.send_impl(item)) }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        // SAFETY:
        //
        // We can acquire a mutable reference without checking as:
        //
        // - This type is !Sync and !Send.
        // - This function is not used by any other functions and hence uniquely owns the
        // mutable reference.
        // - The mutable reference is dropped at the end of this function.
        unsafe { self.inner.with_mut(|inner| inner.close_impl(true)) }
    }
}

/// Creates a oneshot channel.
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Rc::new(UnsafeCell::new(Inner {
        rx_waker: None,
        closed: false,
        item: None,

        _marker: PhantomData,
    }));

    (
        Sender {
            inner: inner.clone(),
        },
        Receiver { inner },
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::sync::Barrier;
    use tokio::task::{spawn_local, LocalSet};
    use tokio::test;
    use tokio::time::sleep;

    use super::*;

    #[test]
    async fn oneshot_works() {
        let (tx, rx) = channel();

        tx.send(0).expect("failed to send.");

        assert_eq!(rx.await.expect("failed to receive."), 0);
    }

    #[test]
    async fn oneshot_drops_sender() {
        let local_set = LocalSet::new();

        local_set
            .run_until(async {
                let (tx, rx) = channel::<usize>();

                spawn_local(async move {
                    sleep(Duration::from_millis(1)).await;

                    drop(tx);
                });
                rx.await.expect_err("successful to receive.");
            })
            .await;
    }

    #[test]
    async fn oneshot_drops_receiver() {
        let local_set = LocalSet::new();

        local_set
            .run_until(async {
                let (tx, rx) = channel::<usize>();

                let bar = Arc::new(Barrier::new(2));

                {
                    let bar = bar.clone();
                    spawn_local(async move {
                        sleep(Duration::from_millis(1)).await;

                        drop(rx);

                        bar.wait().await;
                    });
                }

                bar.wait().await;

                tx.send(0).expect_err("successful to send.");
            })
            .await;
    }
}
