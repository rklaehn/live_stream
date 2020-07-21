use futures::prelude::*;
use std::{
    cell::RefCell,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

fn foo<'a>(c: &'a mut u64) -> impl Stream<Item = u64> + 'a {
    stream::unfold(10u64, move |max| {
        future::ready(if *c < max {
            *c += 1;
            Some((*c, max))
        } else {
            None
        })
    })
}

fn foo2(arg: u64) -> impl Stream<Item = u64> {
    let snd: Rc<RefCell<Option<u64>>> = Rc::new(RefCell::new(None));
    let rcv = snd.clone();
    let mut fut = async move {
        let mut arg = arg;
        let mut stream = foo(&mut arg);
        while let Some(item) = stream.next().await {
            *snd.borrow_mut() = Some(item);
            yield_now().await;
        }
    }
    .boxed_local();
    let mut done = false;
    futures::stream::poll_fn(move |ctx| {
        if done {
            return Poll::Ready(None);
        }
        match fut.poll_unpin(ctx) {
            Poll::Pending => {
                if let Some(item) = rcv.borrow_mut().take() {
                    Poll::Ready(Some(item))
                } else {
                    Poll::Pending
                }
            }
            Poll::Ready(_) => {
                done = true;
                if let Some(item) = rcv.borrow_mut().take() {
                    Poll::Ready(Some(item))
                } else {
                    Poll::Ready(None)
                }
            }
        }
    })
}

async fn test_foo_foo2() {
    let stream = foo2(5);
    assert_eq!(stream.collect::<Vec<_>>().await, vec![6, 7, 8, 9, 10]);
}

pub async fn yield_now() {
    YieldNow(false).await
}

struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    // The futures executor is implemented as a FIFO queue, so all this future
    // does is re-schedule the future back to the end of the queue, giving room
    // for other futures to progress.
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.0 {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}
