![Step Function I/O](./sfio_logo.png)

Commercial library by [Step Function I/O](https://stepfunc.io/)

# rodbus

[Rust](https://www.rust-lang.org/) async/await implementation of the [Modbus](http://www.modbus.org/) protocol using
[Tokio](https://tokio.rs/) with idiomatic bindings for C/C++, Java, and .NET Core.

The library supports Modbus TCP, RTU, and TLS including role-based access control using the X.509 role identifier in the Modbus security specification.

## License

Refer to [`LICENSE`](./LICENSE) for the terms of the non-commercial license.  This software is "source available", but is not
"open source". You must purchase a commercial license to use this software for profit.

## Library

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

## Bindings

Bindings in C, C++, .NET Core, and Java are available for this library. See the
[documentation](https://stepfunc.io/products/libraries/modbus/) for more details.

## Modbus client CLI

You can test Modbus servers from the command line with the
[rodbus-client](https://crates.io/crates/rodbus-client) crate.
