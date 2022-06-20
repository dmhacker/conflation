#[cfg(test)]
mod sync_tests {
    use conflation::sync::mpmc::unbounded;
    use std::collections::HashMap;
    use std::sync::mpsc::{RecvError, RecvTimeoutError, SendError, TryRecvError};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn recv_has_no_conflations() {
        let (tx, rx) = unbounded();
        // Simple send and recv's
        assert_eq!(tx.send(1, "test 1".to_owned()), Ok(()));
        assert_eq!(rx.recv(), Ok((1, "test 1".to_owned())));
        assert_eq!(tx.send(2, "test 2".to_owned()), Ok(()));
        assert_eq!(rx.recv(), Ok((2, "test 2".to_owned())));
        // Send two at once
        assert_eq!(tx.send(8888, "foobar".to_owned()), Ok(()));
        assert_eq!(tx.send(9999, "amazing".to_owned()), Ok(()));
        assert_eq!(rx.recv(), Ok((8888, "foobar".to_owned())));
        assert_eq!(rx.recv(), Ok((9999, "amazing".to_owned())));
    }

    #[test]
    fn try_recv_has_single_conflation() {
        let (tx, rx) = unbounded();
        // All insertions correspond to a single key
        for i in 0..1000 {
            assert_eq!(tx.send(1337, format!("test {i}").to_owned()), Ok(()));
        }
        let mut received = HashMap::new();
        let mut counts = HashMap::new();
        loop {
            match rx.try_recv() {
                Ok((key, value)) => {
                    received.insert(key.clone(), value);
                    let counter = counts.entry(key).or_insert(0);
                    *counter += 1;
                }
                Err(TryRecvError::Empty) => {
                    break;
                }
                Err(TryRecvError::Disconnected) => {
                    assert!(false);
                }
            }
        }
        assert_eq!(received.len(), 1);
        assert_eq!(received.get(&1337), Some(&"test 999".to_owned()));
        assert_eq!(counts.len(), 1);
        assert_eq!(counts.get(&1337), Some(&1));
    }

    #[test]
    fn try_recv_has_many_conflations() {
        let (tx, rx) = unbounded();
        // Add duplicate insertions for each index
        for j in 0..4 {
            for i in 0..1000 {
                assert_eq!(tx.send(i, format!("test {i} {j}").to_owned()), Ok(()));
            }
        }
        let mut received = HashMap::new();
        let mut counts = HashMap::new();
        loop {
            match rx.try_recv() {
                Ok((key, value)) => {
                    received.insert(key.clone(), value);
                    let counter = counts.entry(key).or_insert(0);
                    *counter += 1;
                }
                Err(TryRecvError::Empty) => {
                    break;
                }
                Err(TryRecvError::Disconnected) => {
                    assert!(false);
                }
            }
        }
        assert!(tx.len() == 0);
        assert!(!tx.is_disconnected());
        assert!(rx.len() == 0);
        assert!(!rx.is_disconnected());
        assert_eq!(received.len(), 1000);
        assert_eq!(counts.len(), 1000);
        for i in 0..1000 {
            assert_eq!(received.get(&i), Some(&format!("test {i} 3").to_owned()));
            assert_eq!(counts.get(&i), Some(&1));
        }
    }

    #[test]
    fn try_iter_has_many_conflations() {
        // Same test as in `try_recv_has_many_conflations`, only that the
        // iterator is used instead of directly calling `try_recv` in a loop
        let (tx, rx) = unbounded();
        for j in 0..4 {
            for i in 0..1000 {
                assert_eq!(tx.send(i, format!("test {i} {j}").to_owned()), Ok(()));
            }
        }
        let mut received = HashMap::new();
        let mut counts = HashMap::new();
        for (key, value) in rx.try_iter() {
            received.insert(key.clone(), value);
            let counter = counts.entry(key).or_insert(0);
            *counter += 1;
        }
        assert!(tx.len() == 0);
        assert!(!tx.is_disconnected());
        assert!(rx.len() == 0);
        assert!(!rx.is_disconnected());
        assert_eq!(received.len(), 1000);
        assert_eq!(counts.len(), 1000);
        for i in 0..1000 {
            assert_eq!(received.get(&i), Some(&format!("test {i} 3").to_owned()));
            assert_eq!(counts.get(&i), Some(&1));
        }
    }

    #[test]
    fn recv_timeout_times_out() {
        let (_tx, rx) = unbounded::<i64, i64>();
        assert_eq!(
            rx.recv_timeout(Duration::from_millis(100)),
            Err(RecvTimeoutError::Timeout)
        );
    }

    #[test]
    fn recv_deadline_times_out() {
        let (_tx, rx) = unbounded::<i64, i64>();
        // Deadline in the past sets a timeout of 0
        assert_eq!(
            rx.recv_deadline(Instant::now() - Duration::from_millis(100)),
            Err(RecvTimeoutError::Timeout)
        );
        assert_eq!(
            rx.recv_deadline(Instant::now() + Duration::from_millis(100)),
            Err(RecvTimeoutError::Timeout)
        );
    }

    #[test]
    fn sender_immediately_disconnects() {
        let (tx, rx) = unbounded::<i64, i64>();
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        drop(tx);
        assert_eq!(
            rx.recv_timeout(Duration::from_millis(100)),
            Err(RecvTimeoutError::Disconnected)
        );
        assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
        assert_eq!(rx.recv(), Err(RecvError));
    }

    #[test]
    fn sender_clones_gradually_disconnect() {
        let (tx, rx) = unbounded::<i64, i64>();
        let tx1 = tx.clone();
        let tx2 = tx.clone();
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        drop(tx);
        drop(tx2);
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        drop(tx1);
        assert_eq!(
            rx.recv_timeout(Duration::from_millis(100)),
            Err(RecvTimeoutError::Disconnected)
        );
        assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
        assert_eq!(rx.recv(), Err(RecvError));
    }

    #[test]
    fn receiver_immediately_disconnects() {
        let (tx, rx) = unbounded();
        drop(rx);
        assert_eq!(tx.send(1, 2), Err(SendError((1, 2))));
    }

    #[test]
    fn receiver_moved_to_worker_thread() {
        let (tx, rx) = unbounded();
        let handle = thread::spawn(move || {
            assert!(!rx.is_disconnected());
            loop {
                match rx.recv() {
                    Err(_) => break,
                    _ => (),
                }
            }
            assert!(rx.is_disconnected());
        });
        for i in 0..10000 {
            tx.send(i, "some string".to_owned()).unwrap();
        }
        assert!(!tx.is_disconnected());
        drop(tx);
        handle.join().unwrap();
    }

    #[test]
    fn receiver_is_disconnected() {
        let (tx, rx) = unbounded();
        assert!(!tx.is_disconnected());
        assert!(!rx.is_disconnected());
        assert!(tx.len() == 0);
        assert!(rx.len() == 0);
        tx.send(1, 1).unwrap();
        tx.send(1, 2).unwrap();
        assert!(!tx.is_disconnected());
        assert!(!rx.is_disconnected());
        assert!(tx.len() == 1);
        assert!(rx.len() == 1);
        drop(tx);
        assert!(rx.is_disconnected());
        assert!(rx.len() == 1);
        rx.recv().unwrap();
        assert!(rx.is_disconnected());
        assert!(rx.len() == 0);
    }
}
