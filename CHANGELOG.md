### 1.6.0 ###
* :shield: **Hardened against panics.** Lints deny common sources of panics, and CI proves that parsing, answering requests and formatting replies can't panic for any input.
* :bell: **License update.** The license now prohibits using the library to build or test other implementations of the protocol, including by mining it with AI tools or using it as an oracle, and reserves AI training. See [#224](https://github.com/stepfunc/rodbus/pull/224).
* :bell: **`MinTlsVersion` is a minimum.** `V1_2` now allows TLS 1.3, and `V1_3` no longer allows TLS 1.2. See [#212](https://github.com/stepfunc/rodbus/pull/212).
* :bell: A request's timeout covers sending it; one that can't be sent in time fails with `Io(TimedOut)` and the connection is reopened. See [#215](https://github.com/stepfunc/rodbus/pull/215).
* :bell: A read whose range is empty or runs past address 0xFFFF fails with `InvalidRange` before anything is sent. See [#235](https://github.com/stepfunc/rodbus/pull/235).
* :bell: Out-of-range settings, such as a zero timeout or queue size or reversed retry bounds, are adjusted to valid values. See [#236](https://github.com/stepfunc/rodbus/pull/236), [#241](https://github.com/stepfunc/rodbus/pull/241) and [#302](https://github.com/stepfunc/rodbus/pull/302).
* :bell: Servers ignore requests for units they don't serve and validate requests as the standard requires. See [#274](https://github.com/stepfunc/rodbus/pull/274), [#277](https://github.com/stepfunc/rodbus/pull/277), [#280](https://github.com/stepfunc/rodbus/pull/280) and [#284](https://github.com/stepfunc/rodbus/pull/284).
* :bell: RTU readers resynchronize after a bad frame instead of reopening the port, and frames are separated by the required silence. See [#269](https://github.com/stepfunc/rodbus/pull/269), [#297](https://github.com/stepfunc/rodbus/pull/297) and [#345](https://github.com/stepfunc/rodbus/pull/345).
* :bell: Ordinary traffic no longer logs warnings; received frames log under `rodbus::common::frame`. See [#242](https://github.com/stepfunc/rodbus/pull/242), [#243](https://github.com/stepfunc/rodbus/pull/243) and [#346](https://github.com/stepfunc/rodbus/pull/346).
* :shield: TLS client handshakes time out after 5 seconds, and a request's write is bounded by its timeout. See [#215](https://github.com/stepfunc/rodbus/pull/215).
* :shield: Bindings: a TLS server without an authorization handler applies its address filter, and roles containing NUL are refused. See [#220](https://github.com/stepfunc/rodbus/pull/220) and [#221](https://github.com/stepfunc/rodbus/pull/221).
* :shield: Each file attached to a GitHub release has a build provenance attestation that `gh attestation verify` checks, and the jars on Maven Central are signed with a new key, `DFD7 6A43 C349 DBF5 46AB 9033 C6FB 909E 94CB 1619`. See [#384](https://github.com/stepfunc/rodbus/pull/384).
* :star: `Channel::shutdown()` and `ServerHandle::shutdown()` end a task while handles are still alive. See [#198](https://github.com/stepfunc/rodbus/pull/198).
* :star: Non-spawning `create_*_server_task()` functions return a `ServerTask` for the caller to spawn. See [#188](https://github.com/stepfunc/rodbus/pull/188).
* :star: Configuration and task types implement `Debug`, without key material. See [#240](https://github.com/stepfunc/rodbus/pull/240).
* :star: Prebuilt C/C++ bindings for 32-bit x86 Linux (`i686-unknown-linux-gnu`). See [#370](https://github.com/stepfunc/rodbus/pull/370).
* :bug: The client ignores responses from a unit it didn't address. See [#223](https://github.com/stepfunc/rodbus/pull/223).
* :bug: IPv4 address filters match IPv4-mapped peers on dual-stack servers. See [#260](https://github.com/stepfunc/rodbus/pull/260).
* :bug: Bindings: a single coil write returns the `WriteHandler`'s exception instead of always `ILLEGAL_DATA_ADDRESS`. See [#364](https://github.com/stepfunc/rodbus/pull/364).
* :bug: Other fixes. See [#218](https://github.com/stepfunc/rodbus/pull/218), [#222](https://github.com/stepfunc/rodbus/pull/222) and [#247](https://github.com/stepfunc/rodbus/pull/247).
* :wrench: `Channel` request methods take `&self`. See [#191](https://github.com/stepfunc/rodbus/pull/191).
* :wrench: The prebuilt C/C++ and .NET library is about 25% smaller. See [#298](https://github.com/stepfunc/rodbus/pull/298) and [#319](https://github.com/stepfunc/rodbus/pull/319).
* :wrench: The prebuilt bindings use AWS-LC for TLS on every platform, which adds post-quantum key exchange to the musl and 32-bit ARM libraries. See [#359](https://github.com/stepfunc/rodbus/pull/359).
* :book: The guide is reorganized and corrected, with the same examples in every language. See [#362](https://github.com/stepfunc/rodbus/pull/362) and [#366](https://github.com/stepfunc/rodbus/pull/366).

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
