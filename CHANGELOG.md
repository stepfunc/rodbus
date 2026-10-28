### 1.6.0 ###
* :shield: **Hardened against panics.** Lints deny common sources of panics throughout the library, and CI now **proves** that parsing what it receives, answering requests and formatting what it sends can't panic for any input. The [security page](https://docs.stepfunc.io/rodbus/1.6.0/guide/docs/security) has the details. See [#244](https://github.com/stepfunc/rodbus/pull/244), [#349](https://github.com/stepfunc/rodbus/pull/349), [#350](https://github.com/stepfunc/rodbus/pull/350) and [#365](https://github.com/stepfunc/rodbus/pull/365).
* :bell: **License update.** The license now prohibits using the library to build or test other implementations of the protocol, including by mining it with AI tools or using it as an oracle, and reserves AI training. See [#224](https://github.com/stepfunc/rodbus/pull/224).
* :bell: **`MinTlsVersion` is applied as a minimum.** `V1_2` allows TLS 1.2 and 1.3, and `V1_3` only TLS 1.3. Before, `V1_2` refused TLS 1.3 and `V1_3` allowed TLS 1.2. See [#212](https://github.com/stepfunc/rodbus/pull/212).
* :bell: A request's timeout covers sending it as well as receiving the response. A request that can't be sent in time fails with `RequestError::Io(TimedOut)`, and the connection or serial port is reopened. See [#215](https://github.com/stepfunc/rodbus/pull/215).
* :bell: A read whose range is empty or runs past address 0xFFFF fails with `InvalidRange` before anything is sent. See [#235](https://github.com/stepfunc/rodbus/pull/235).
* :bell: Settings out of range, such as a zero response timeout or queue size, or a retry strategy with reversed bounds, are adjusted to valid values instead of failing later. See [#236](https://github.com/stepfunc/rodbus/pull/236), [#241](https://github.com/stepfunc/rodbus/pull/241) and [#302](https://github.com/stepfunc/rodbus/pull/302).
* :bell: Servers ignore every request for a unit they don't serve, and check request values, quantities and byte counts as the standard requires. See [#274](https://github.com/stepfunc/rodbus/pull/274), [#277](https://github.com/stepfunc/rodbus/pull/277), [#280](https://github.com/stepfunc/rodbus/pull/280) and [#284](https://github.com/stepfunc/rodbus/pull/284).
* :bell: RTU: frames are kept apart by the required silence on the line, readers resynchronize after a bad frame instead of reopening the port, and a broadcast write completes once its turnaround delay has passed. See [#269](https://github.com/stepfunc/rodbus/pull/269), [#297](https://github.com/stepfunc/rodbus/pull/297) and [#345](https://github.com/stepfunc/rodbus/pull/345).
* :bell: Ordinary traffic no longer logs warnings, and received frames are logged with the target `rodbus::common::frame`. See [#242](https://github.com/stepfunc/rodbus/pull/242), [#243](https://github.com/stepfunc/rodbus/pull/243) and [#346](https://github.com/stepfunc/rodbus/pull/346).
* :shield: TLS clients time out a handshake after 5 seconds, and every request's write is bounded by its timeout, so a stalled peer can't hold up a channel. See [#215](https://github.com/stepfunc/rodbus/pull/215).
* :shield: Bindings: a TLS server created without an authorization handler applies its address filter, and a certificate role containing a NUL character is refused. See [#220](https://github.com/stepfunc/rodbus/pull/220) and [#221](https://github.com/stepfunc/rodbus/pull/221).
* :star: Add `Channel::shutdown()` and `ServerHandle::shutdown()` to end the associated task while handles are still alive. See [#198](https://github.com/stepfunc/rodbus/pull/198).
* :star: Add non-spawning `create_tcp_server_task()`, `create_rtu_server_task()`, `create_tls_server_task()` and `create_tls_server_task_with_authz()`, which return a `ServerTask` for the caller to spawn. See [#188](https://github.com/stepfunc/rodbus/pull/188).
* :star: `Debug` for `ClientOptions`, `ChannelLoggingMode`, `ClientTask`, `ServerTask`, `MaybeAsync`, `TlsClientConfig` and `TlsServerConfig`, without key material. See [#240](https://github.com/stepfunc/rodbus/pull/240).
* :star: Prebuilt C/C++ bindings for 32-bit x86 Linux (`i686-unknown-linux-gnu`). See [#370](https://github.com/stepfunc/rodbus/pull/370).
* :bug: The client ignores a response from a unit other than the one it addressed, so a late reply on a multi-drop line isn't taken as the answer. See [#223](https://github.com/stepfunc/rodbus/pull/223).
* :bug: IPv4 address filters match IPv4-mapped peers, so a server bound to `[::]` accepts the IPv4 clients a filter allows. See [#260](https://github.com/stepfunc/rodbus/pull/260).
* :bug: Bindings: a server answers a single coil write with the exception the `WriteHandler` returns; it always answered `ILLEGAL_DATA_ADDRESS`. See [#364](https://github.com/stepfunc/rodbus/pull/364).
* :bug: Other fixes. See [#218](https://github.com/stepfunc/rodbus/pull/218), [#222](https://github.com/stepfunc/rodbus/pull/222) and [#247](https://github.com/stepfunc/rodbus/pull/247).
* :wrench: Client `Channel` read and write methods and `set_decode_level()` take `&self` instead of `&mut self`. Existing callers compile unchanged. See [#191](https://github.com/stepfunc/rodbus/pull/191).
* :wrench: The prebuilt C/C++ and .NET library is about 25% smaller, and the Java library smaller too. See [#298](https://github.com/stepfunc/rodbus/pull/298) and [#319](https://github.com/stepfunc/rodbus/pull/319).
* :wrench: The prebuilt bindings use AWS-LC for TLS on every platform. The musl and 32-bit ARM Linux libraries switch from *ring*, which adds post-quantum key exchange and makes them 0.7 to 2.2 MB larger. See [#359](https://github.com/stepfunc/rodbus/pull/359).
* :wrench: The RTU CRC is computed by rodbus itself, which drops the `crc` dependency. See [#365](https://github.com/stepfunc/rodbus/pull/365).
* :wrench: The `serialization` feature builds on its own. See [#207](https://github.com/stepfunc/rodbus/pull/207).
* :book: A security page in the guide, consistent examples in every language, a curated guide in the stepfunc.io style, and documentation corrected where it disagreed with the library. See [#206](https://github.com/stepfunc/rodbus/pull/206), [#304](https://github.com/stepfunc/rodbus/pull/304), [#322](https://github.com/stepfunc/rodbus/pull/322), [#341](https://github.com/stepfunc/rodbus/pull/341), [#361](https://github.com/stepfunc/rodbus/pull/361), [#362](https://github.com/stepfunc/rodbus/pull/362), [#363](https://github.com/stepfunc/rodbus/pull/363), [#366](https://github.com/stepfunc/rodbus/pull/366) and [#368](https://github.com/stepfunc/rodbus/pull/368).

### 1.5.0 ###
* :star: Add non-spawning `create_tcp_client_task_with_options()`, `create_tls_client_task_with_options()`, and `create_rtu_client_task()` that return a `ClientTask` for the caller to spawn, enabling custom task instrumentation. See [#180](https://github.com/stepfunc/rodbus/pull/180).
* :lock: Update `rustls-webpki` to 0.103.13 to resolve [RUSTSEC-2026-0098](https://rustsec.org/advisories/RUSTSEC-2026-0098.html), [RUSTSEC-2026-0099](https://rustsec.org/advisories/RUSTSEC-2026-0099.html), and [RUSTSEC-2026-0104](https://rustsec.org/advisories/RUSTSEC-2026-0104.html). The two name-constraint advisories (0098, 0099) are theoretically reachable during TLS handshake but require certificate misissuance to exploit; the CRL panic (0104) is unreachable as Rodbus does not use CRLs.
* :star: TCP client performance optimizations: ~33% fewer heap allocations and ~3.4% fewer instructions per request, including an iterative MBAP parser and `ExactSizeIterator` impls for the response iterators. See [#177](https://github.com/stepfunc/rodbus/pull/177).
* :bug: Fix a `ReadBuffer` shift condition that, in a rare buffer-full edge case, caused a spurious disconnect (no data corruption). See [#177](https://github.com/stepfunc/rodbus/pull/177).
* :star: Add configurable limit on consecutive client response timeouts. See [#166](https://github.com/stepfunc/rodbus/pull/166).
* :wrench: Use aws-lc-rs crypto provider for TLS on most platforms. See [#164](https://github.com/stepfunc/rodbus/pull/164).
* :wrench: Tunable connection logging to reduce verbosity during disrupted communication. See [#163](https://github.com/stepfunc/rodbus/pull/163).
* :star: Add `spawn_tcp_client_task_with_options()` with extensible `ClientOptions` builder API. See [#161](https://github.com/stepfunc/rodbus/pull/161).
* :bug: Fix bad default upper limit in retry strategy. See [#158](https://github.com/stepfunc/rodbus/pull/158).
* :wrench: PartialEq and Debug consistently applied to public API. See [#157](https://github.com/stepfunc/rodbus/pull/157).
* :star: Support serde on configuration types with new `serialization` feature. See [#156](https://github.com/stepfunc/rodbus/pull/156).
* :wrench: rodbus-client modernization with serial support and improved error handling. See [#152](https://github.com/stepfunc/rodbus/pull/152).

### 1.4.0 ###
* :wrench: Avoid task spawning in client FFI methods. See [#136](https://github.com/stepfunc/rodbus/pull/136).
* :wrench: Requests in the client API now fail immediately while the client is connecting.

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
