### 1.3.1 ###
* :bug: Fix issue with master channels not properly exiting and thrashing CPU. See [#120](https://github.com/stepfunc/rodbus/issues/120).

### 1.3.0 ###
* :wrench: Update to rustls 0.21 which allows peer names with IP addresses in the SAN extension.
* :wrench: Move common TLS configuration to its own crate shared with our Modbus library.
* :star: PEM parser now supports extracting PKCS#1 private keys, i.e. PEM files with `BEGIN RSA PRIVATE KEY`.
* :book: Documentation improvements in the bindings via [oo-bindgen 0.8.3](https://github.com/stepfunc/oo_bindgen/blob/main/CHANGELOG.md).

### 1.2.0 ###
* :star: Add a mechanism to the bindings to shut down the Runtime with a timeout. See [#110](https://github.com/stepfunc/rodbus/pull/110).

### 1.1.0 ###
* :star: Enable TCP_NODELAY for client and server sockets. See [#99](https://github.com/stepfunc/rodbus/pull/99).
* :star: Enable full link-time optimization (LTO) in release builds. See [#103](https://github.com/stepfunc/rodbus/pull/103).
* :star: Add support for 3 MUSL Linux targets to C/C++ and .NET. See [#104](https://github.com/stepfunc/rodbus/pull/104).
* :star: Use only dependencies from crates.io allowing first release there. See [#106](https://github.com/stepfunc/rodbus/pull/106).
* :star: Internal refactoring to promote code reuse with DNP3. See: [#100](https://github.com/stepfunc/rodbus/pull/100), [#101](https://github.com/stepfunc/rodbus/pull/101), [#102](https://github.com/stepfunc/rodbus/pull/102).

### 1.0.0 ###
* :star: Add Modbus Security (TLS) support. See [#52](https://github.com/stepfunc/rodbus/pull/52).
* :star: Add RTU support. See [#56](https://github.com/stepfunc/rodbus/pull/56).
* :star: Dynamic protocol decoding. See [#61](https://github.com/stepfunc/rodbus/pull/66).
* :star: Resolve host names on client. See [#68](https://github.com/stepfunc/rodbus/pull/68).
* :star: Add communication channel state callbacks. See [#77](https://github.com/stepfunc/rodbus/issues/77).
* :star: TCP/TLS server can now filter incoming connections based on IP. See [#87](https://github.com/stepfunc/rodbus/pull/87).
* :bug: Properly reset TCP connection retry timeout on success. See [#82](https://github.com/stepfunc/rodbus/issues/82).

### 0.9.1 ###
* Client callbacks are now not blocking.
  See [#53](https://github.com/stepfunc/rodbus/pull/53).
* :bug: Fix leak of `tracing::Span` in bindings.
  See [#53](https://github.com/stepfunc/rodbus/pull/53).
* :star: Add Linux AArch64 support in Java and .NET.
  See [#51](https://github.com/stepfunc/rodbus/pull/51).

### 0.9.0 ###
* :tada: First official release
