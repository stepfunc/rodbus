use oo_bindgen::native_enum::NativeEnum;
use oo_bindgen::{BindingError, Handle, LibraryBuilder};

pub(crate) fn define_exception(
    lib: &mut LibraryBuilder,
) -> Result<Handle<NativeEnum>, BindingError> {
    lib.define_native_enum("Exception")?
        .variant("IllegalFunction", 0x01, "The data address received in the query is not an allowable address for the server")?
        .variant("IllegalDataAddress", 0x02, "The data address received in the query is not an allowable address for the server")?
        .variant("IllegalDataValue", 0x03, "A value contained in the request is not an allowable value for server")?
        .variant("ServerDeviceFailure", 0x04, "An unrecoverable error occurred while the server was attempting to perform the requested action")?
        .variant("Acknowledge", 0x05, "Specialized use in conjunction with  programming commands. The server has accepted the request and is processing it.")?
        .variant("ServerDeviceBusy", 0x06, "Specialized use in conjunction with  programming commands. The server is engaged in processing a long-duration program command, try again later")?
        .variant("MemoryParityError", 0x08, "Specialized use in conjunction with function codes 20 and 21 and reference type 6, to indicate that the extended file area failed to pass a consistency check. The server attempted to read a record file, but detected a parity error in the memory")?
        .variant("GatewayPathUnavailable", 0x0A, "Specialized use in conjunction with gateways, indicates that the gateway was unable to allocate an internal communication path from the input port to the output port for processing the request. Usually means that the gateway is mis-configured or overloaded")?
        .variant("GatewayTargetDeviceFailedToRespond", 0x0B, "Specialized use in conjunction with gateways, indicates that no response was obtained from the target device. Usually means that the device is not present on the network")?
        .variant("Unknown", 0xFF, "The status code is not defined in the Modbus specification, refer to the raw exception code to see what the server sent")?
        .doc("Exception values from the Modbus specification")?
        .build()
}

pub(crate) fn define_status(lib: &mut LibraryBuilder) -> Result<Handle<NativeEnum>, BindingError> {
    lib.define_native_enum("Status")?
        .variant(
            "Ok",
            0,
            "The operation was successful and any return value may be used",
        )?
        .variant(
            "Shutdown",
            1,
            "The channel was shutdown before the operation could complete",
        )?
        .variant(
            "NoConnection",
            2,
            "No connection could be made to the server",
        )?
        .variant(
            "ResponseTimeout",
            3,
            "No valid response was received before the timeout",
        )?
        .variant("BadRequest", 4, "The request was invalid")?
        .variant(
            "BadResponse",
            5,
            "The response from the server was received but was improperly formatted",
        )?
        .variant(
            "IOError",
            6,
            "An I/O error occurred on the underlying stream while performing the request",
        )?
        .variant(
            "BadFraming",
            7,
            "A framing error was detected while performing the request",
        )?
        .variant(
            "Exception",
            8,
            "The server returned an exception code (see separate exception value)",
        )?
        .variant(
            "InternalError",
            9,
            "An unspecified internal error occurred while performing the request",
        )?
        .doc("Status returned during synchronous and asynchronous API calls")?
        .build()
}
