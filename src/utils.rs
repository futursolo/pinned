use std::cell::Cell;
use std::task::Poll;

use futures::future::poll_fn;

/// Skip one round of execution.
pub(crate) async fn yield_now() {
    let yielded = Cell::new(false);

    poll_fn(move |cx| {
        if yielded.get() {
            return Poll::Ready(());
        }

        yielded.set(true);

        cx.waker().wake_by_ref();
        Poll::Pending
    })
    .await;
}
