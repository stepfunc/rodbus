Rodbus-FFI is a C compatibility layer on top of the [Rodbus](https://github.com/automatak/rodbus) library written in 
Rust. It provides an idiomatic C API with both synchronous (blocking) and asynchronous (non-blocking callbacks) function
variants for performing Modbus client operations.

This first release provides only the TCP client side of the underlying Rust crate. The server side API will be added
in an upcoming release.

All of the documentation you need is found in the rodbus.h header. There are C examples in the
[rodbus-ffi/cmake/examples](https://github.com/automatak/rodbus/tree/master/rodbus-ffi/cmake/examples) directory.