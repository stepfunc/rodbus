# rodbus

[![crates.io](https://img.shields.io/crates/v/rodbus.svg)](https://crates.io/crates/rodbus)
[![docs.rs](https://docs.rs/rodbus/badge.svg)](https://docs.rs/rodbus)
![MSRV](https://img.shields.io/badge/rustc-1.39+-blue.svg) [![Build status](https://github.com/automatak/rodbus/workflows/CI/badge.svg)](https://github.com/automatak/rodbus/actions)
[![Codecov](https://codecov.io/gh/automatak/rodbus/graph/badge.svg)](https://codecov.io/gh/automatak/rodbus)
![License](https://img.shields.io/github/license/automatak/rodbus)

[Rust](https://www.rust-lang.org/) async/await implementation of the [Modbus](http://www.modbus.org/) protocol using
[Tokio](https://tokio.rs/) with seamless C/C++ interoperability.

## Library

[Documentation](https://docs.rs/rodbus)

The library provides a simple interface to send Modbus requests. Using Rust's powerful type system,
the library is safe, memory efficient and easy to use. All of the error handling in the library
is explicit and logging is also available. Three client interfaces are provided for making requests:

- Async (Rust futures)
- Callback-based
- Synchronous (blocking)

The library also provides a Modbus server API requiring the user to implement a single trait to handle requests.
The interface is designed to minimize copying of data making the server efficient. The [`server`](./rodbus/examples/server.rs)
example shows a simple server implementation.

The following function codes are supported:
- Read Coils (`0x01`)
- Read Discrete Inputs (`0x02`)
- Read Holding Registers (`0x03`)
- Read Input Registers (`0x04`)
- Write Single Coil (`0x05`)
- Write Single Register (`0x06`)
- Write Multiple Coils (`0x0F`)
- Write Multiple Registers (`0x10`)

Under the hood, the library uses an event loop that can efficiently utilize all the
system resources. The [`perf`](./rodbus/examples/perf.rs) example is a benchmark that
creates multiple sessions on a single server and sends multiple requests in parallel.
On a decent workstation, the benchmark achieved around 200k requests per second spread
across 100 concurrent sessions in only 800 KB of memory.

## C/C++ bindings

The [rodbus-ffi](./rodbus-ffi) directory contains C/C++ bindings to the library.
Requests can be sent asynchronously using a callback mechanism or synchronously
with blocking function calls.

To generate the bindings, do the following:
- Install `cbindgen` with `cargo install cbindgen`
- Run `cbingen -c cmake/cbindgen.c.toml -o rodbus.h`
- Run `cbingen -c cmake/cbindgen.cpp.toml -o rodbus.hpp`
- Build `rodbus-ffi`

To use the bindings, you will need to include the `prelude.h` file and `rodbus.h`
or `rodbus.hpp`. You will also need to link with the compiled library
`rodbus_ffi.[dll|so]` found in the target directory.

There is also a [CMake script](./rodbus-ffi/cmake/CMakeLists.txt) that can help
you automatically build and link to rodbus from a C/C++ project.

## Modbus client CLI

The [rodbus-client](./rodbus-client) directory contains a Modbus client application for
testing Modbus servers from the the command line. You can run it with `cargo run -p rodbus-client [...]`.
For general detailed help, run `cargo run -p rodbus-client -- help`.

Use the `-h` option to specify the host to connect to and the `-i` option to
specify the Modbus unit ID.

Each request can be sent using the following subcommands:

- `rc`: read coils
    - `-s`: starting address
    - `-q`: quantity of coils
- `rdi`: read discrete inputs
    - `-s`: starting address
    - `-q`: quantity of discrete inputs
- `rhr`: read holding registers
    - `-s`: starting address
    - `-q`: quantity of holding registers
- `rir`: read input registers
    - `-s`: starting address
    - `-q`: quantity of input registers
- `wsc`: write single coil
    - `-i`: index of the coil
    - `-v`: value of the coil (`true` or `false`)
- `wsr`: write single register
    - `-i`: index of the register
    - `-v`: value of the register
- `wmc`: write multiple coils
    - `-s`: starting address
    - `-v`: values of the coils (e.g. 10100011)
- `wmr`: write multiple registers
    - `-s`: starting address
    - `-v`: values of the registers as a comma delimited list (e.g. 1,4,7)

Examples:

- Read coils 10 to 19 on `localhost`, port 502, unit ID `0x02`: `cargo run -p rodbus-client -- -h
  127.0.0.1:502 -i 2 rc -s 10 -q 10`
- Read holding registers 10 to 19: `cargo run -p rodbus-client -- rhr -s 10 -q 10`
- Write coil 10: `cargo run -p rodbus-client -- wsc -i 10 -v true`
- Write multiple coils: `cargo run -p rodbus-client -- wmc -s 10 -v 101001`
- Write register 10: `cargo run -p rodbus-client -- wsr -i 10 -v 76`
- Write 42 to registers 10, 11 and 12: `cargo run -p rodbus-client -- wmr -s 10
  -v 42,42,42`

It is also possible to send periodic requests with the `-p` argument. For example,
to send a read coils request every 2 seconds, you would do this:
`cargo run -p rodbus-client -- -p 2000 rc -s 10 -q 10`

## License

Licensed under the 3-Clause BSD License. See [LICENSE.md](./LICENSE.md) for more
details.

Copyright 2019-2020 Automatak LLC. All rights reserved.
