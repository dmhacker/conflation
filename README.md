# conflation

A set of concurrent channels in Rust that conflates submitted items via sender-supplied keys.

## Overview

A conflating queue is generally useful when two conditions are met:
1. A producer is enqueueing items at a faster rate than a slower consumer can dequeue them.
2. It is acceptable to drop in-flight messages by retaining only the most recently produced message along some (key) boundary.

An **important** note is that these channels do not offer the strong guarantee that 
replacement will always occur if a new keyed entry is inserted. Instead, the producer is
simply allowed to eliminate prior entries if necessary. The receiver should
generally assume that unread priors are allowable within a keyed section of the queue.

## Synchronous Usage

The semantics of these channels are identical to standard `Sender`s and `Receiver`s,
only that a key is required to be specified upon submission. The key and value are
returned by the receiver.

```rust
use conflation::sync::mpmc::unbounded;

let (tx, rx) = bounded();
// Both messages are tagged with key = 1
let tx_result1 = tx.send(1, "foo".to_owned()).unwrap();
let tx_result2 = tx.send(1, "bar".to_owned()).unwrap();
// The received key will always be 1.
// However, the receiver may yield either "foo" or "bar", as (1, "foo") may be conflated in-flight.
let (key, value) = rx.recv().unwrap();
...
```

Unlike `std::sync::mpsc` channels, `conflation::sync::mpmc` channels are
multi-producer and multi-consumer. If multiple receivers are used, then this channel
will function identically to a work-stealing queue, only that work submitted
with the same key can optionally be conflated in transit.

## Planned Additions

* Iterators over synchronous unbounded channels
* Support for asynchronous unbounded channels
* Support for asynchronous bounded channels