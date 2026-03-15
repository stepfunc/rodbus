# TCP Client (Master) Performance Analysis

Analysis of the TCP client hot path focusing on read/write operations from user code through serialization, TCP I/O, deserialization, and response delivery.

## 1. Sequential Request Processing (Largest Architectural Win)

**Location:** `rodbus/src/client/task.rs:257-299` (`execute_request`)

The current design processes exactly **one request at a time**: serialize → write → wait for response → parse → next. The full network round-trip latency is paid per request. For a typical polling application reading 20 different address ranges, this means 20 serial round-trips.

Modbus TCP's transaction ID exists specifically to support pipelining. The code already matches on `tx_id` in the response loop (line 285-291), so the protocol-level plumbing is partially there.

**Possible approach:** Allow N requests in-flight simultaneously. The `ClientLoop` would maintain a small map of `TxId → Request`, send multiple requests before waiting for responses, and match responses back by transaction ID. This could improve throughput by up to Nx where N is the pipeline depth, since you amortize the RTT across N requests.

**Risk:** Some Modbus servers may not handle pipelined requests well. This would need to be opt-in.

**Estimated impact:** High — removes RTT-per-request bottleneck.

## 2. Double Heap Allocation Per Request (Box + Oneshot)

**Location:** `rodbus/src/client/channel.rs:106-117`, `rodbus/src/client/message.rs:258-306`

Every `Channel::read_*` / `Channel::write_*` call creates:
1. A `tokio::sync::oneshot::channel()` (heap allocation)
2. A `Box<dyn Callback>` wrapping a closure that captures the oneshot sender (heap allocation)

So **2 heap allocations per request** just for the response plumbing. The `Promise<T>` type in `message.rs:258` boxes the callback unconditionally.

**Fix:** Store a `PromiseInner` enum that can be either `Oneshot(oneshot::Sender<Result<T, RequestError>>)` or `Boxed(Box<dyn Callback<T>>)`. The `Channel` API would use the `Oneshot` variant directly, avoiding the Box. The `CallbackSession` (FFI path) continues to use `Boxed`. This removes one allocation from the Rust API hot path.

Similarly, `rodbus/src/client/requests/read_bits.rs:17-19` and `rodbus/src/client/requests/read_registers.rs:20-22` have their own `Promise` types that also box unconditionally.

**Estimated impact:** Medium — removes 1 heap allocation per request.

## 3. Vec Allocation on Every Read Response

**Location:** `rodbus/src/client/requests/read_bits.rs:69`, `rodbus/src/client/requests/read_registers.rs:72`

```rust
let _ = tx.send(x.map(|x| x.collect()));
```

Every read response allocates a new `Vec<Indexed<bool>>` or `Vec<Indexed<u16>>`. For polling applications that read the same register set thousands of times per second, this is a steady stream of allocations.

**Fix options:**
- Provide an API that accepts a pre-allocated `&mut Vec<Indexed<T>>` which gets cleared and refilled.
- Provide an API returning an iterator over borrowed data (the `CallbackSession` already does this with `BitIterator`/`RegisterIterator` which are zero-copy, but the public `Channel` API forces the collect).

**Estimated impact:** Medium — removes 1 heap allocation per read response.

## 4. Tracing Span Per Transaction

**Location:** `rodbus/src/client/task.rs:224-226`

```rust
.instrument(tracing::info_span!("Transaction", tx_id = %tx_id))
```

This creates a tracing span for **every** Modbus transaction. Even with no subscriber, the span creation and the `%tx_id` formatting (`Display::fmt` → `write!(f, "{:#04X}", self.value)`) have nonzero cost. At thousands of requests per second, this adds up.

**Fix:** Guard the instrumentation behind the decode level check, or use `tracing::enabled!()` to skip span creation when no subscriber is active at the relevant level.

**Estimated impact:** Low-Medium — saves formatting + span bookkeeping per request.

## 5. Response Frame Copy

**Location:** `rodbus/src/tcp/frame.rs:77-85` (`parse_body`)

```rust
fn parse_body(header, adu_length, cursor) -> Result<Frame, RequestError> {
    let mut frame = Frame::new(header);  // zeroes 253 bytes
    frame.set(cursor.read(adu_length)?); // copies payload into frame
    Ok(frame)
}
```

Every response:
1. Zero-initializes a 253-byte `[u8; MAX_ADU_LENGTH]` array in `Frame::new()`
2. Copies the response payload into it

This could be avoided by having the response parser work directly from the `ReadBuffer` data (borrowing rather than copying). The `Frame` struct exists to own the data across the parse boundary, but the data is consumed immediately after in `handle_response`.

**Estimated impact:** Low — saves 253-byte zero-init + payload memcpy per response.

## 6. ReadBuffer Shift Logic Bug (Correctness)

**Location:** `rodbus/src/common/buffer.rs:94`

```rust
if self.end == self.len() {
```

`self.len()` is `self.end - self.begin`. So this condition reduces to `self.begin == 0`, meaning the shift only triggers when data is already at the beginning (i.e., a no-op). The correct condition should be `self.end == self.buffer.len()` to detect when the write position has reached buffer capacity and consumed bytes at the front need to be shifted down.

In practice this is benign for Modbus because the empty-check above resets indices between frames and responses typically fit in a single read. But it is a latent bug if a partial read leaves consumed bytes in the buffer.

**Estimated impact:** N/A — correctness fix, not performance.

## 7. ReadBuffer u16 Parsing

**Location:** `rodbus/src/common/buffer.rs:68-72`

```rust
pub(crate) fn read_u16_be(&mut self) -> Result<u16, InternalError> {
    let b1 = self.read_u8()? as u16;
    let b2 = self.read_u8()? as u16;
    Ok((b1 << 8) | b2)
}
```

Two separate bounds-checked reads instead of one. Could read 2 bytes at once and use `u16::from_be_bytes()`. Very minor but this is in the frame parsing hot path (called 3 times per MBAP header).

**Estimated impact:** Negligible.

## Summary

| # | Optimization | Type | Est. Impact |
|---|---|---|---|
| 1 | Request pipelining | Architectural | **High** |
| 2 | Eliminate Box in Promise for Channel API | Allocation | **Medium** |
| 3 | Reusable Vec / zero-copy read API | Allocation | **Medium** |
| 4 | Conditional tracing span creation | CPU | **Low-Medium** |
| 5 | Zero-copy response parsing (avoid Frame copy) | Copy | **Low** |
| 6 | ReadBuffer shift condition fix | Correctness | N/A |
| 7 | Batch u16 reads in buffer | CPU | **Negligible** |

The dominant bottleneck is #1 (sequential processing). For a typical Modbus polling application with many address ranges, pipelining would dwarf all other optimizations combined since the cost is dominated by network RTT, not CPU.
