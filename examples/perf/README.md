# Performance Test

Loopback performance test that spins up an in-process TCP server and client, then measures
request throughput over a configurable duration.

## Usage

```bash
cargo build --release -p example-perf
./target/release/example-perf [OPTIONS]
```

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `-s` | Number of parallel sessions | 1 |
| `-c` | Duration in seconds | 5 |
| `-l` | Enable tracing/logging | false |
| `-p` | TCP port | 40000 |

## Profiling with Valgrind

Valgrind provides deterministic, reproducible measurements that are independent of system load.

### Instruction counts (callgrind)

Measures total CPU instructions executed. Useful for detecting changes in serialization,
parsing, and other CPU-bound code paths.

```bash
valgrind --tool=callgrind ./target/release/example-perf -s 1 -c 1
```

Produces a `callgrind.out.<pid>` file. Use `callgrind_annotate` to inspect:

```bash
callgrind_annotate callgrind.out.<pid>
```

### Allocation profiling (dhat)

Counts heap allocations, bytes allocated, and allocation lifetimes per call site.
Useful for verifying that allocation-reduction optimizations have the expected effect.

```bash
valgrind --tool=dhat --dhat-out-file=/tmp/dhat.out ./target/release/example-perf -s 1 -c 1
```

To view results, open `/usr/libexec/valgrind/dh_view.html` in a browser and load the output file.
