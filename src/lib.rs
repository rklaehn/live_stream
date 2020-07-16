use futures::prelude::*;
use std::{
    fmt::Debug,
    pin::Pin,
    task::{Context, Poll},
};

pub struct LiveStream<D, S> {
    _data: Box<D>,
    stream: S,
}

impl<D, S: Debug> Debug for LiveStream<D, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveStream")
            .field("stream", &self.stream)
            .finish()
    }
}

impl<'a, D: 'a, S: 'a> LiveStream<D, S> {
    pub fn new<F: Fn(&'a D) -> S>(d: D, f: F) -> Self {
        let d = Box::new(d);
        let dr = unsafe { std::mem::transmute::<&D, &'a D>(d.as_ref()) };
        let stream = f(dr);
        // nobody must touch the contents of the box from here!
        Self { _data: d, stream }
    }

    pub fn new_mut<F: Fn(&'a mut D) -> S>(d: D, f: F) -> Self {
        let mut d = Box::new(d);
        let dr = unsafe { std::mem::transmute::<&mut D, &'a mut D>(d.as_mut()) };
        let stream = f(dr);
        // nobody must touch the contents of the box from here!
        Self { _data: d, stream }
    }
}

impl<'a, A, B: Stream + Unpin> Stream for LiveStream<A, B> {
    type Item = B::Item;
    fn poll_next(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.stream.poll_next_unpin(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream_to_10<'a>(c: &'a mut u64) -> impl Stream<Item = u64> + 'a {
        stream::unfold(10u64, move |max| {
            future::ready(if *c < max {
                *c += 1;
                Some((*c, max))
            } else {
                None
            })
        })
    }

    fn stream_to<'a>(max: &'a u64) -> impl Stream<Item = u64> + 'a {
        stream::unfold(0u64, move |c| {
            future::ready(if c < *max { Some((c + 1, c + 1)) } else { None })
        })
    }

    #[tokio::test]
    async fn test_mut() {
        let stream = LiveStream::new_mut(1, |c| stream_to_10(c));
        let items = stream.collect::<Vec<_>>().await;
        assert_eq!(items, vec![2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }

    #[tokio::test]
    async fn test_ref() {
        let stream = LiveStream::new(10, |c| stream_to(c));
        let items = stream.collect::<Vec<_>>().await;
        assert_eq!(items, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }
}
