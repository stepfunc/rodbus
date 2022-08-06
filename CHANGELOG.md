### Next (1.0.0) ###
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
