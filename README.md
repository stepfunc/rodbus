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

Rodbus is library for implementing [Modbus](https://modbus.org/) client and server applications. The library is safe, 
memory-efficient and easy to use. All of the error handling in the library is explicit and logging is available by
providing a backend to the [log](https://crates.io/crates/log) crate. Three client interfaces are provided for making requests:

- Async (Rust futures)
- Callback-based
- Synchronous (blocking)

The [`client`](./rodbus/examples/client.rs) and [`server`](./rodbus/examples/server.rs) examples demonstrate simple
usage of the API.

The following function codes are supported:
- Read Coils (`0x01`)
- Read Discrete Inputs (`0x02`)
- Read Holding Registers (`0x03`)
- Read Input Registers (`0x04`)
- Write Single Coil (`0x05`)
- Write Single Register (`0x06`)
- Write Multiple Coils (`0x0F`)
- Write Multiple Registers (`0x10`)

The library uses the Tokio executor under the hood. The [`perf`](./rodbus/examples/perf.rs) example is a benchmark that
creates multiple sessions on a single server and sends multiple requests in parallel. On a decent workstation,
the benchmark achieved around 200k requests per second spread across 100 concurrent sessions in only 800 KB of memory.

## Future support

* [Modbus Security](http://modbus.org/docs/MB-TCP-Security-v21_2018-07-24.pdf) using TLS
* [Modbus RTU](http://modbus.org/docs/PI_MBUS_300.pdf) (serial)

## C/C++ bindings

The [rodbus-ffi](./rodbus-ffi) directory contains an idiomatic C/C++ API to the library.
Requests can be sent asynchronously using callback functions or synchronously with blocking function calls.

In this early release, only the client side of the library has been exposed and is only known to work on *nix platforms.
Please read the  [C/C++ Documentation](https://docs.automatak.com/rodbus) and review the [examples](./rodbus-ffi/cmake/examples).

To generate the bindings, do the following:
- Install `cbindgen` with `cargo install cbindgen`
- Run `cbingen -c cmake/cbindgen.c.toml -o rodbus.h`
- Run `cbingen -c cmake/cbindgen.cpp.toml -o rodbus.hpp`
- Build `rodbus-ffi`

To use the bindings, you will need to include`rodbus.h` or `rodbus.hpp` which each include `prelude.h`. 
You will also need to link with the compiled library `rodbus_ffi.so` found in the target directory.

There is also a [CMake script](./rodbus-ffi/cmake/CMakeLists.txt) that can help you automatically build and link to
rodbus from a C/C++ project.

## Modbus client CLI

You can test Modbus servers from the command line with the [rodbus-client](https://crates.io/crates/rodbus-client) crate.

## License

Licensed under the GPL version 3. See [LICENSE.md](./LICENSE.md) for more details.

Copyright 2019-2020 Automatak LLC. All rights reserved.