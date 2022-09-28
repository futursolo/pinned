use std::marker::PhantomData;

use crate::cell::UnsafeCell;
use crate::oneshot;

/// A `BarrierWaitResult` is returned by [`wait`](Barrier::wait) when all tasks in the [`Barrier`]
/// have rendezvoused.
#[derive(Clone, Debug)]
pub struct BarrierWaitResult(bool);

impl BarrierWaitResult {
    /// Returns `true` if this task from wait is the "leader task".
    ///
    /// Only one task will have `true` returned from their result, all other tasks will have
    /// `false` returned.
    pub fn is_leader(&self) -> bool {
        self.0
    }
}

#[derive(Debug)]
struct Inner {
    wakers: Vec<oneshot::Sender<()>>,
    n: usize,

    // This type is not send or sync.
    _marker: PhantomData<*const ()>,
}

impl Inner {
    fn wait_impl(&mut self) -> Option<oneshot::Receiver<()>> {
        let should_wake = self.n - self.wakers.len() == 1;

        if should_wake {
            for sender in self.wakers.drain(..) {
                let _ = sender.send(());
            }

            None
        } else {
            let (tx, rx) = oneshot::channel();

            self.wakers.push(tx);

            Some(rx)
        }
    }
}

/// A barrier enables multiple tasks to synchronise the beginning of some computation.
///
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// # let local_set = tokio::task::LocalSet::new();
/// # local_set.run_until(async {
/// # use std::rc::Rc;
/// # use pinned::Barrier;
/// #
/// let mut handles = Vec::with_capacity(10);
/// let barrier = Rc::new(Barrier::new(10));
/// for _ in 0..10 {
///     let c = barrier.clone();
///     // The same messages will be printed together.
///     // You will NOT see any interleaving.
///     handles.push(tokio::task::spawn_local(async move {
///         println!("before wait");
///         let wait_result = c.wait().await;
///         println!("after wait");
///         wait_result
///     }));
/// }
///
/// // Will not resolve until all "after wait" messages have been printed
/// let mut num_leaders = 0;
/// for handle in handles {
///     let wait_result = handle.await.unwrap();
///     if wait_result.is_leader() {
///         num_leaders += 1;
///     }
/// }
///
/// // Exactly one barrier will resolve as the "leader"
/// assert_eq!(num_leaders, 1);
/// # }).await;
/// # }
/// ```
#[derive(Debug)]
pub struct Barrier {
    inner: UnsafeCell<Inner>,
}

impl Barrier {
    /// Creates a new barrier that can block a given number of tasks.
    ///
    /// A barrier will block `n`-1 tasks which call [`Barrier::wait`] and then wake up all tasks at
    /// once when the `n`th task calls `wait`.
    pub fn new(n: usize) -> Self {
        Self {
            inner: UnsafeCell::new(Inner {
                wakers: Vec::with_capacity(n - 1),
                n,

                _marker: PhantomData,
            }),
        }
    }

    /// Does not resolve until all tasks have rendezvoused here.
    ///
    /// Barriers are re-usable after all tasks have rendezvoused once, and can be used continuously.
    ///
    /// A single (arbitrary) future will receive a [`BarrierWaitResult`] that returns `true` from
    /// [`BarrierWaitResult::is_leader`] when returning from this function, and all other tasks will
    /// receive a result that will return `false` from `is_leader`.
    pub async fn wait(&self) -> BarrierWaitResult {
        // SAFETY:
        //
        // We can acquire a mutable reference without checking as:
        //
        // - This type is !Sync and !Send.
        // - This function is not used by any other functions and hence uniquely owns the
        // mutable reference.
        // - The mutable reference is dropped at the end of this function.
        let maybe_recv = unsafe { self.inner.with_mut(|inner| inner.wait_impl()) };

        match maybe_recv {
            Some(m) => {
                m.await.expect("channel failed");
                BarrierWaitResult(false)
            }
            None => BarrierWaitResult(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use futures::channel::mpsc::unbounded;
    use futures::stream::StreamExt;
    use tokio::task::{spawn_local, LocalSet};
    use tokio::test;

    use super::*;

    #[test]
    async fn test_barrier() {
        let local_set = LocalSet::new();

        local_set
            .run_until(async {
                const N: usize = 10;

                let barrier = Rc::new(Barrier::new(N));
                let (tx, mut rx) = unbounded();

                for _ in 0..N - 1 {
                    let c = barrier.clone();
                    let tx = tx.clone();
                    spawn_local(async move {
                        tx.unbounded_send(c.wait().await.is_leader()).unwrap();
                    });
                }

                // At this point, all spawned tasks should be blocked,
                // so we shouldn't get anything from the port
                assert!(rx.try_next().is_err());

                let mut leader_found = barrier.wait().await.is_leader();

                // Now, the barrier is cleared and we should get data.
                for _ in 0..N - 1 {
                    if rx.next().await.unwrap() {
                        assert!(!leader_found);
                        leader_found = true;
                    }
                }
                assert!(leader_found);
            })
            .await;
    }
}
