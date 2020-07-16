//! # LiveStream - lifetime extender for streams
//!
//! Let's say you got a database that can stream results. For efficiency reasons, the query method just
//! takes a reference to the database, a reference to the query, and returns a lifetime limited stream.
//!
//! ```
//!use futures::prelude::*;
//!# struct Database;
//!# struct Query;
//! impl Database {
//!     fn query<'a>(&'a mut self, query: &'a Query) -> impl Stream<Item = String> + 'a
//!#    { futures::stream::empty() }
//! }
//! ```
//!
//! This looks very sophisticated and works great in the examples directory. However, now you have to
//! put the database in production. And that entails putting it behind a warp based server. Unfortunately,
//! now you need a self-contained stream without unduly limited lifetime.
//!
//! One solution would involve changing the signature of the query method and using lots of `.clone()` for
//! the query and the database object.
//!
//! With this crate it is possible to package the values (Query and Database) and the function in a single
//! struct that itself implements Stream without lifetime restriction.
//!
//! ```
//!use futures::prelude::*;
//!use live_stream::LiveStream;
//!# struct Database;
//!# struct Query;
//! impl Database {
//!#     fn query<'a>(&'a mut self, query: &'a Query) -> impl Stream<Item = String> + 'a
//!#     { futures::stream::empty() }
//!
//!     fn query_static(self, query: Query) -> impl Stream<Item = String> + 'static {
//!          LiveStream::new_mut((self, query), |(db, q)| db.query(q) )
//!     }
//! }
//! ```

use futures::prelude::*;
use std::{
    fmt::Debug,
    pin::Pin,
    task::{Context, Poll},
};

/// A stream that is built from a piece of data an a function that takes a reference to said data
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
    /// take ownership of some data and call a fn with a reference to the data
    ///
    /// Will produce a self-contained stream
    pub fn new<F: Fn(&'a D) -> S>(d: D, f: F) -> Self {
        let d = Box::new(d);
        // extend the lifetime of the reference to as long as the box will live
        let dr = unsafe { std::mem::transmute::<&D, &'a D>(d.as_ref()) };
        let stream = f(dr);
        // nobody must touch the contents of the box from here!
        Self { _data: d, stream }
    }

    /// take ownership of some data and call a fn with a mutable reference to the data
    ///
    /// Will produce a self-contained stream
    pub fn new_mut<F: Fn(&'a mut D) -> S>(d: D, f: F) -> Self {
        let mut d = Box::new(d);
        // extend the lifetime of the reference to as long as the box will live
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

    struct Database;
    struct Query;

    impl Database {
        /// efficient query without cloning the database or query object
        fn query<'a>(&'a mut self, _query: &'a Query) -> impl Stream<Item = String> + 'a {
            futures::stream::iter((1..=3).map(|x| x.to_string()))
        }

        fn query_static(self, query: Query) -> impl Stream<Item = String> + 'static {
            LiveStream::new_mut((self, query), |(db, q)| db.query(q))
        }
    }

    #[tokio::test]
    async fn test_example() {
        let db = Database;
        let q = Query;
        let res = db.query_static(q).collect::<Vec<_>>().await;
        assert_eq!(res, vec!["1", "2", "3"]);
    }
}
