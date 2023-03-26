#[cfg(test)]
mod async_tests {
    use conflation::{unbounded, RecvFuture};
    use pin_project::pin_project;
    use std::future::Future;
    use std::hash::Hash;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use std::thread;
    use std::time::Duration;

    // #[futures_test::test]
    // async fn recv_has_no_conflations() {
    //     let (tx, rx) = unbounded();
    //     // Simple send and recv's
    //     assert_eq!(tx.send(1, "test 1".to_owned()), Ok(()));
    //     assert_eq!(rx.recv_async().await, Ok((1, "test 1".to_owned())));
    //     assert_eq!(tx.send(2, "test 2".to_owned()), Ok(()));
    //     assert_eq!(rx.recv(), Ok((2, "test 2".to_owned())));
    //     // Send two at once
    //     assert_eq!(tx.send(8888, "foobar".to_owned()), Ok(()));
    //     assert_eq!(tx.send(9999, "amazing".to_owned()), Ok(()));
    //     assert_eq!(rx.recv(), Ok((8888, "foobar".to_owned())));
    //     assert_eq!(rx.recv(), Ok((9999, "amazing".to_owned())));
    //     let handle = thread::spawn(move || {
    //         std::thread::sleep(Duration::from_millis(100));
    //         assert_eq!(tx.send(8888, "from different thread".to_owned()), Ok(()))
    //     });
    //     assert_eq!(
    //         rx.recv_async().await,
    //         Ok((8888, "from different thread".to_owned()))
    //     );
    //     handle.join().unwrap();
    // }

    #[pin_project]
    struct PollOnceFuture<'a, 'b, K, V> {
        future: RecvFuture<'a, 'b, K, V>,
    }

    impl<'a, 'b, K, V> Future for PollOnceFuture<'a, 'b, K, V>
    where
        K: Hash,
        K: Eq,
    {
        type Output = Poll<<RecvFuture<'a, 'b, K, V> as Future>::Output>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let mut this = self.project();
            Poll::Ready(Pin::new(&mut this.future).poll(cx))
        }
    }

    #[futures_test::test]
    async fn recv_can_drop_without_completing() {
        let (tx, rx) = unbounded();
        // Receive future is manually polled once, then dropped;
        // its signaller should be removed from the queue
        {
            let once_fut = PollOnceFuture {
                future: rx.recv_async(),
            };
            assert!(matches!(once_fut.await, Poll::Pending));
        }
        // Receive to completion; the signaller from the interrupted
        // future should be removed earlier, so sending to the future should
        // wake the second future and not the first
        let handle = thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            assert_eq!(tx.send(8888, "from different thread".to_owned()), Ok(()))
        });
        assert_eq!(
            rx.recv_async().await,
            Ok((8888, "from different thread".to_owned()))
        );
        handle.join().unwrap();
    }
}
