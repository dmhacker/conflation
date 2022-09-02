# conflation

A set of thread-safe channels in Rust that conflates submitted items via sender-supplied keys.

## Overview

A conflating queue is generally useful when two conditions are met:
1. A producer is enqueueing items at a faster rate than a slower consumer can dequeue them.
2. It is acceptable to drop in-flight messages by retaining only the most recently produced message along some (key) boundary.

This channel offers the **strong** guarantee that a replacement will always occur
if a new keyed entry is inserted into the channel and an unread duplicate is already present.
In this case, the duplicate will be dropped, and the new entry will take its place at the
back of the queue.

Under the hood, the channel mechanism uses a linked hash map to perform conflation.
This generally makes the channel slower than its `std` equivalents for workloads without
many duplicate keys. Consider using this channel only when conflation is appropriate
(e.g. to help with backpressure).

## Usage

The semantics of these channels are identical to standard `Sender`s and `Receiver`s,
only that a key is required to be specified upon submission. The key and value are
returned by the receiver.

```rust
use conflation::unbounded;

let (tx, rx) = unbounded();
// Both messages are tagged with key = 1
let tx_result1 = tx.send(1, "foo".to_owned()).unwrap();
let tx_result2 = tx.send(1, "bar".to_owned()).unwrap();
// The receiver will only yield (1, "bar") as (1, "foo") was
// conflated by the producer.
let (key, value) = rx.recv().unwrap();
```

Unlike `std::sync::mpsc` channels, `conflation::sync::mpmc` channels are
multi-producer and multi-consumer. If multiple receivers are used, then this channel
will function identically to a work-sharing queue, only that work submitted
with the same key can potentially be conflated in transit.