# conflation

A set of concurrent channels in Rust that conflates submitted items via sender-supplied keys.

## Overview

A conflating queue is generally useful when two conditions are met:
1. A producer is enqueueing items at a faster rate than a slower consumer can dequeue them.
2. It is acceptable to drop in-flight messages by retaining only the most recently produced message along some (key) boundary.

An example where such a situation might arise is with a trading system. 
A slower consumer might hook into a higher-frequency market data stream, but at any given point
might only care about the most recent price for a particular ticker symbol on some interval. Rather
than continually pausing the producer to handle backpressure, one could instead have the producer
conflate prices in its outgoing queue with the key being equivalent to the symbol.

An **important** note is that these channels do not offer the strong guarantee that 
replacement will always occur if a new keyed entry is inserted. Instead, the producer is
simply allowed to eliminate prior entries if necessary. The receiver should
generally assume that unread priors are allowable within a keyed section of the queue.

## Usage

The semantics of these channels are identical to standard `Sender`s and `Receiver`s,
only that a key is required to be specified upon submission. The key and value are
returned by the receiver.

```rust
let (mut tx, mut rx) = channel();
// Both messages are tagged with key = 1
let tx_result1 = tx.send(1, "foo".to_owned()).unwrap();
let tx_result2 = tx.send(1, "bar".to_owned()).unwrap();
// The received key will always be 1.
// However, the receiver may yield either "foo" or "bar", as (1, "foo") may be conflated in-flight.
let (key, value) = rx.recv().unwrap();
...
```

## Roadmap

Currently, only synchronous versions of the channels are supplied. In the future,
this will be extended to support asynchronous versions as well.
