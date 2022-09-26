//! A multi-producer, single-receiver channel.
//!
//! This is an asynchronous, `!Send` version of `std::sync::mpsc`. Currently only the unbounded
//! variant is implemented.
//!
//! [`UnboundedReceiver`] implements [`Stream`] and allows asynchronous tasks to read values out of
//! the channel. The `UnboundedReceiver` Stream will suspend and wait for available values if the
//! current queue is empty. [`UnboundedSender`] implements [`Sink`] and allows messages to be sent
//! to the corresponding `UnboundedReceiver`. The `UnboundedReceiver` also implements a
//! [`send_now`](UnboundedSender::send_now) method to send a value synchronously.

use std::collections::VecDeque;
use std::marker::PhantomData;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use futures::sink::Sink;
use futures::stream::{FusedStream, Stream};
use thiserror::Error;

use crate::cell::UnsafeCell;

/// Error returned by [`try_next`](UnboundedReceiver::try_next).
#[derive(Error, Debug)]
#[error("queue is empty")]
pub struct TryRecvError {
    _marker: PhantomData<()>,
}

/// Error returned by [`send_now`](UnboundedSender::send_now).
#[derive(Error, Debug)]
#[error("failed to send")]
pub struct SendError<T> {
    /// The send value.
    pub inner: T,
}

/// Error returned by [`UnboundedSender`] when used as a [`Sink`](futures::sink::Sink).
#[derive(Error, Debug)]
#[error("failed to send")]
pub struct TrySendError {
    _marker: PhantomData<()>,
}

#[derive(Debug)]
struct Inner<T> {
    rx_waker: Option<Waker>,
    closed: bool,
    sender_ctr: usize,
    items: VecDeque<T>,

    // This type is not send or sync.
    _marker: PhantomData<Rc<()>>,
}

impl<T> Inner<T> {
    fn close_impl(&mut self) {
        self.closed = true;

        if let Some(ref m) = self.rx_waker {
            m.wake_by_ref();
        }
    }

    #[inline]
    fn try_next_impl(&mut self) -> Result<Option<T>, TryRecvError> {
        match (self.items.pop_front(), self.closed) {
            (Some(m), _) => Ok(Some(m)),
            (None, true) => Ok(None),
            (None, false) => Err(TryRecvError {
                _marker: PhantomData,
            }),
        }
    }

    #[inline]
    fn poll_next_impl(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        match (self.items.pop_front(), self.closed) {
            (Some(m), _) => Poll::Ready(Some(m)),
            (None, false) => {
                self.rx_waker = Some(cx.waker().clone());
                Poll::Pending
            }
            (None, true) => Poll::Ready(None),
        }
    }

    #[inline]
    fn is_terminated_impl(&self) -> bool {
        self.items.is_empty() && self.closed
    }

    #[inline]
    fn send_impl(&mut self, item: T) -> Result<(), SendError<T>> {
        if self.closed {
            return Err(SendError { inner: item });
        }

        self.items.push_back(item);

        if let Some(ref m) = self.rx_waker {
            m.wake_by_ref();
        }

        Ok(())
    }

    #[inline]
    fn pre_clone_sender_impl(&mut self) {
        self.sender_ctr += 1;
    }

    #[inline]
    fn drop_sender_impl(&mut self) {
        let sender_ctr = {
            self.sender_ctr -= 1;
            self.sender_ctr
        };

        if sender_ctr == 0 {
            self.close_impl();
        }
    }
}

/// The receiver of an unbounded mpsc channel.
///
/// This is created by the [`unbounded`] function.
#[derive(Debug)]
pub struct UnboundedReceiver<T> {
    inner: Rc<UnsafeCell<Inner<T>>>,
}

impl<T> UnboundedReceiver<T> {
    /// Try to read the next value from the channel.
    ///
    /// This function will return:
    /// - `Ok(Some(T))` if a value is ready.
    /// - `Ok(None)` if the channel has become closed.
    /// - `Err(TryRecvError)` if the channel is not closed and the channel is empty.
    pub fn try_next(&self) -> Result<Option<T>, TryRecvError> {
        // SAFETY:
        //
        // We can acquire a mutable reference without checking as:
        //
        // - This type is !Sync and !Send.
        // - This function is not used by any other functions and hence uniquely owns the
        // mutable reference.
        // - The mutable reference is dropped at the end of this function.
        unsafe { self.inner.with_mut(|inner| inner.try_next_impl()) }
    }

    /// Closes the receiver of the channel without dropping it.
    ///
    /// This prevents any further messages from being sent on the channel while still enabling the
    /// receiver to drain messages in the buffer.
    pub fn close(&self) {
        // SAFETY:
        //
        // We can acquire a mutable reference without checking as:
        //
        // - This type is !Sync and !Send.
        // - This function is not used by any other functions and hence uniquely owns the
        // mutable reference.
        // - The mutable reference is dropped at the end of this function.

        unsafe { self.inner.with_mut(|inner| inner.close_impl()) }
    }
}

impl<T> Stream for UnboundedReceiver<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // SAFETY:
        //
        // We can acquire a mutable reference without checking as:
        //
        // - This type is !Sync and !Send.
        // - This function is not used by any other functions and hence uniquely owns the
        // mutable reference.
        // - The mutable reference is dropped at the end of this function.
        unsafe { self.inner.with_mut(|inner| inner.poll_next_impl(cx)) }
    }
}

impl<T> FusedStream for UnboundedReceiver<T> {
    fn is_terminated(&self) -> bool {
        // SAFETY:
        //
        // We can acquire a mutable reference without checking as:
        //
        // - This type is !Sync and !Send.
        // - This function is not used by any other functions and hence uniquely owns the
        // mutable reference.
        // - The mutable reference is dropped at the end of this function.
        unsafe { self.inner.with(|inner| inner.is_terminated_impl()) }
    }
}

impl<T> Drop for UnboundedReceiver<T> {
    fn drop(&mut self) {
        // SAFETY:
        //
        // We can acquire a mutable reference without checking as:
        //
        // - This type is !Sync and !Send.
        // - This function is not used by any other functions and hence uniquely owns the
        // mutable reference.
        // - The mutable reference is dropped at the end of this function.
        unsafe { self.inner.with_mut(|inner| inner.close_impl()) }
    }
}

/// The sender of an unbounded mpsc channel.
///
/// This value is created by the [`unbounded`] function.
#[derive(Debug)]
pub struct UnboundedSender<T> {
    inner: Rc<UnsafeCell<Inner<T>>>,
}

impl<T> UnboundedSender<T> {
    /// Sends a value to the unbounded receiver.
    ///
    /// This is an unbounded sender, so this function differs from
    /// [`SinkExt::send`](futures::sink::SinkExt::send) by ensuring the return type reflects
    /// that the channel is always ready to receive messages.
    pub fn send_now(&self, item: T) -> Result<(), SendError<T>> {
        // SAFETY:
        //
        // We can acquire a mutable reference without checking as:
        //
        // - This type is !Sync and !Send.
        // - This function is not used by any function that have already acquired a mutable
        // reference.
        // - The mutable reference is dropped at the end of this function.

        unsafe { self.inner.with_mut(move |inner| inner.send_impl(item)) }
    }

    /// Closes the channel.
    ///
    /// Every sender (dropped or not) is considered closed when this method is called.
    pub fn close_now(&self) {
        // SAFETY:
        //
        // We can acquire a mutable reference without checking as:
        //
        // - This type is !Sync and !Send.
        // - This function is not used by any function that have already acquired a mutable
        // reference.
        // - The mutable reference is dropped at the end of this function.
        unsafe { self.inner.with_mut(|inner| inner.close_impl()) }
    }
}

impl<T> Clone for UnboundedSender<T> {
    fn clone(&self) -> Self {
        // SAFETY:
        //
        // We can acquire a mutable reference without checking as:
        //
        // - This type is !Sync and !Send.
        // - This function is not used by any other functions and hence uniquely owns the
        // mutable reference.
        // - The mutable reference is dropped at the end of this function.
        unsafe { self.inner.with_mut(|inner| inner.pre_clone_sender_impl()) }

        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Drop for UnboundedSender<T> {
    fn drop(&mut self) {
        // SAFETY:
        //
        // We can acquire a mutable reference without checking as:
        //
        // - This type is !Sync and !Send.
        // - This function is not used by any other functions and hence uniquely owns the
        // mutable reference.
        // - The mutable reference is dropped at the end of this function.
        unsafe { self.inner.with_mut(|inner| inner.drop_sender_impl()) }
    }
}

impl<T> Sink<T> for &'_ UnboundedSender<T> {
    type Error = TrySendError;

    fn start_send(self: std::pin::Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        self.send_now(item).map_err(|_| TrySendError {
            _marker: PhantomData,
        })
    }

    fn poll_ready(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        let closed = unsafe { self.inner.with(|inner| inner.closed) };

        match closed {
            false => Poll::Ready(Ok(())),
            true => Poll::Ready(Err(TrySendError {
                _marker: PhantomData,
            })),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        self.close_now();

        Poll::Ready(Ok(()))
    }
}

/// Creates an unbounded channel.
///
/// The `send` method on Senders created by this function will always succeed and return immediately
/// as long as the channel is open.
///
/// # Note
///
/// This channel has an infinite buffer and can run out of memory if the channel is not actively
/// drained.
pub fn unbounded<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let inner = Rc::new(UnsafeCell::new(Inner {
        rx_waker: None,
        closed: false,

        sender_ctr: 1,
        items: VecDeque::new(),
        _marker: PhantomData,
    }));

    (
        UnboundedSender {
            inner: inner.clone(),
        },
        UnboundedReceiver { inner },
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::sink::SinkExt;
    use futures::stream::StreamExt;
    use tokio::task::{spawn_local, LocalSet};
    use tokio::test;
    use tokio::time::sleep;

    use super::*;

    #[test]
    async fn mpsc_works() {
        let local_set = LocalSet::new();

        local_set
            .run_until(async {
                let (tx, mut rx) = unbounded::<usize>();

                spawn_local(async move {
                    for i in 0..10 {
                        (&tx).send(i).await.expect("failed to send.");
                        sleep(Duration::from_millis(1)).await;
                    }
                });

                for i in 0..10 {
                    let received = rx.next().await.expect("failed to receive");

                    assert_eq!(i, received);
                }

                assert_eq!(rx.next().await, None);
            })
            .await;
    }

    #[test]
    async fn mpsc_drops_receiver() {
        let (tx, rx) = unbounded::<usize>();
        drop(rx);

        (&tx).send(0).await.expect_err("should fail to send.");
    }

    #[test]
    async fn mpsc_multi_sender() {
        let local_set = LocalSet::new();

        local_set
            .run_until(async {
                let (tx, mut rx) = unbounded::<usize>();

                spawn_local(async move {
                    let tx2 = tx.clone();

                    for i in 0..10 {
                        if i % 2 == 0 {
                            (&tx).send(i).await.expect("failed to send.");
                        } else {
                            (&tx2).send(i).await.expect("failed to send.");
                        }

                        sleep(Duration::from_millis(1)).await;
                    }

                    drop(tx2);

                    for i in 10..20 {
                        (&tx).send(i).await.expect("failed to send.");

                        sleep(Duration::from_millis(1)).await;
                    }
                });

                for i in 0..20 {
                    let received = rx.next().await.expect("failed to receive");

                    assert_eq!(i, received);
                }

                assert_eq!(rx.next().await, None);
            })
            .await;
    }

    #[test]
    async fn mpsc_drops_sender() {
        let (tx, mut rx) = unbounded::<usize>();
        drop(tx);

        assert_eq!(rx.next().await, None);
    }
}
