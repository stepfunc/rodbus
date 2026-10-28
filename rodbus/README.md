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

# Security

Written in safe Rust, with sources of panics restricted by lints, and framing, request validation, the parsing of
response values, answering requests and formatting what is sent proven panic-free at link time. See the
[security page](https://docs.stepfunc.io/rodbus/1.6.0-RC1/guide/docs/security) of the guide for
what is enforced and proven, and what isn't covered.

# Supported Modes

* TCP, RTU (serial), and Modbus security (TLS) with and without X.509 extension containing the user role.
* Client and server

## Function Codes

The [`client`](https://github.com/stepfunc/rodbus/blob/main/examples/client/src/main.rs) and [`server`](https://github.com/stepfunc/rodbus/blob/main/examples/server/src/main.rs) examples demonstrate simple
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

## License

This library is publicly available under a non-commercial / non-production license.
Refer to [`LICENSE.txt`](https://raw.githubusercontent.com/stepfunc/rodbus/main/LICENSE.txt) for the terms
of this non-commercial license.

This software is publicly available, but is not "open source".
__You must purchase a commercial license to use this software for profit.__

Please inquire about commercial licensing on our website:

[https://stepfunc.io/contact/](https://stepfunc.io/contact/)

### AI-assisted development

Subject to the applicable license, AI assistants are welcome for evaluating, integrating, debugging,
and testing this library and for authorized maintenance. Section 2.6 prohibits using the library,
its source, documentation, tests, binaries, or derived materials to design, develop, specify, test,
validate, or improve any other implementation of a protocol it implements, including through
requirement extraction or use as a reference, oracle, or test peer. This applies regardless of
commercial, open-source, private, or competitive status, subject to the exceptions in the agreement.
The license does not permit redistributing the software, including sending it to an AI service for
any of those purposes. Text and data mining rights, including for AI training, are reserved subject
to the permissions and exceptions stated in Section 3.3. This summary does not amend the license. The `LICENSE.txt`
included with each release governs that release; the latest versions are
[`LICENSE.txt`](https://github.com/stepfunc/rodbus/blob/main/LICENSE.txt) and
[`AGENTS.md`](https://github.com/stepfunc/rodbus/blob/main/AGENTS.md).

# Cargo Features

Default features, which can be disabled at compile time:
* `tls` - Build the library with support for TLS (secure Modbus), using rustls with the *ring* crypto provider
* `serial` - Build the library with support for Modbus RTU and serial ports

Optional features:
* `tls-aws-lc` - TLS support with the AWS-LC crypto provider instead of *ring*. It can't be combined with `tls`, so disable
  the default features to use it
* `serialization` - Add [serde](https://docs.rs/crate/serde/latest) (de)serialization support for public configuration types

Bindings in C, C++, java, and .NET Core are available for this library. See the
[documentation](https://stepfunc.io/products/libraries/modbus/) for more details.

