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
        .doc("Exception values from the Modbus specification")?
        .build()
}
