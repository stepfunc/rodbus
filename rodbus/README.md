Commercial library by [Step Function I/O](https://stepfunc.io/)

A high-performance implementation of the [Modbus](http://modbus.org/) protocol using [Tokio](https://docs.rs/tokio) and Rust's `async/await` syntax.

# Features

* Panic-free parsing
* Correctness and compliance to the specification
* Built-in logging and protocol decoding
* Automatic connection management with configurable reconnect strategy
* Scalable performance using Tokio's multi-threaded executor
* TLS is implemented using [rustls](https://github.com/rustls/rustls) not openssl
* Model-generated bindings for C, C++, Java, and .NET Core
* Runs on all platforms and operating systems supported by the [Tokio](https://tokio.rs/) runtime:
  - Official support for: Windows x64 and Linux x64, AArch64, ARMv7 and ARMv6
  - Unofficial support: MacOS, PowerPC, MIPS, FreeBSD, and others

# Supported Modes

* TCP, RTU (serial), and Modbus security (TLS) with and without X.509 extension containing the user role.
* Client and server

# Future Support
* Additional function code support as requested by customers

## License

Refer to [`LICENSE`](https://github.com/stepfunc/rodbus/blob/main/LICENSE.txt) for the terms of the non-commercial license.  This software is "source available", but is not
"open source". You must purchase a commercial license to use this software for profit.

## Library

The [`client`](./examples/client.rs) and [`server`](./examples/server.rs) examples demonstrate simple
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

# Cargo Features

Default features can be disabled at compile time:
* `tls` - Build the library with support for TLS (secure Modbus)
* `serial` - Build the library with support for Modbus RTU and serial ports

## Bindings

Bindings in C, C++, java, and .NET Core are available for this library. See the
[documentation](https://stepfunc.io/products/libraries/modbus/) for more details.

