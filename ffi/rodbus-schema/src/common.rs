use oo_bindgen::class::ClassHandle;
use oo_bindgen::error_type::{ErrorType, ExceptionType};
use oo_bindgen::iterator::IteratorHandle;
use oo_bindgen::native_enum::NativeEnumHandle;
use oo_bindgen::native_function::{ReturnType, Type};
use oo_bindgen::native_struct::{NativeStructHandle, StructElementType};
use oo_bindgen::{BindingError, LibraryBuilder};

pub(crate) struct CommonDefinitions {
    pub(crate) error_type: ErrorType,
    pub(crate) decode_level: NativeStructHandle,
    pub(crate) runtime_handle: ClassHandle,
    pub(crate) error_info: NativeStructHandle,
    pub(crate) address_range: NativeStructHandle,
    pub(crate) request_param: NativeStructHandle,
    pub(crate) bit: NativeStructHandle,
    pub(crate) register: NativeStructHandle,
    pub(crate) bit_iterator: IteratorHandle,
    pub(crate) register_iterator: IteratorHandle,
    pub(crate) exception: NativeEnumHandle,
    pub(crate) serial_port_settings: NativeStructHandle,
}

impl CommonDefinitions {
    pub(crate) fn build(lib: &mut LibraryBuilder) -> Result<CommonDefinitions, BindingError> {
        let error_type = build_error_type(lib)?;
        let decode_level = crate::logging::define(lib, error_type.clone())?;
        let bit = build_bit(lib)?;
        let register = build_register(lib)?;
        let exception = crate::enums::define_exception(lib)?;

        Ok(Self {
            error_type: error_type.clone(),
            decode_level,
            runtime_handle: crate::runtime::build_runtime_class(lib, error_type)?,
            error_info: build_error_info(lib, &exception)?,
            address_range: build_address_range(lib)?,
            request_param: build_request_param(lib)?,
            bit: bit.clone(),
            register: register.clone(),
            bit_iterator: build_iterator(lib, &bit)?,
            register_iterator: build_iterator(lib, &register)?,
            exception,
            serial_port_settings: build_serial_params(lib)?,
        })
    }
}

fn build_error_type(lib: &mut LibraryBuilder) -> Result<ErrorType, BindingError> {
    lib.define_error_type(
        "ParamError",
        "ParamException",
        ExceptionType::UncheckedException,
    )?
    .add_error("NullParameter", "Null parameter")?
    .add_error(
        "LoggingAlreadyConfigured",
        "Logging can only be configured once",
    )?
    .add_error("RuntimeCreationFailure", "Failed to create tokio runtime")?
    .add_error("RuntimeDestroyed", "Runtime was already disposed of")?
    .add_error(
        "RuntimeCannotBlockWithinAsync",
        "Runtime cannot execute blocking call within asynchronous context",
    )?
    .add_error("InvalidSocketAddress", "Invalid socket address")?
    .add_error("InvalidRange", "Invalid Modbus address range")?
    .add_error("InvalidRequest", "Invalid Modbus request")?
    .add_error("InvalidIndex", "Invalid index")?
    .add_error(
        "ServerBindError",
        "Server failed to bind to the specified port",
    )?
    .add_error(
        "InvalidUnitId",
        "The specified unit id is not associated to this server",
    )?
    .doc("Error type used throughout the library")?
    .build()
}

fn build_bit(lib: &mut LibraryBuilder) -> Result<NativeStructHandle, BindingError> {
    let bit = lib.declare_native_struct("Bit")?;
    lib.define_native_struct(&bit)?
        .add("index", Type::Uint16, "index of bit")?
        .add("value", Type::Bool, "value of the bit")?
        .doc("index/value tuple of a bit type")?
        .build()
}

fn build_register(lib: &mut LibraryBuilder) -> Result<NativeStructHandle, BindingError> {
    let bit = lib.declare_native_struct("Register")?;
    lib.define_native_struct(&bit)?
        .add("index", Type::Uint16, "index of register")?
        .add("value", Type::Uint16, "value of the register")?
        .doc("index/value tuple of a register type")?
        .build()
}

fn build_address_range(lib: &mut LibraryBuilder) -> Result<NativeStructHandle, BindingError> {
    let info = lib.declare_native_struct("AddressRange")?;
    let info = lib
        .define_native_struct(&info)?
        .add("start", Type::Uint16, "Starting address of the range")?
        .add("count", Type::Uint16, "Number of addresses in the range")?
        .doc("Range of 16-bit addresses")?
        .build()?;

    Ok(info)
}

fn build_request_param(lib: &mut LibraryBuilder) -> Result<NativeStructHandle, BindingError> {
    let param = lib.declare_native_struct("RequestParam")?;
    let param = lib
        .define_native_struct(&param)?
        .add("unit_id", Type::Uint8, "Modbus address for the request")?
        .add(
            "timeout_ms",
            Type::Uint32,
            "Response timeout for the request in milliseconds",
        )?
        .doc("Address and timeout parameters for requests")?
        .build()?;

    Ok(param)
}

fn build_error_info(
    lib: &mut LibraryBuilder,
    exception: &NativeEnumHandle,
) -> Result<NativeStructHandle, BindingError> {
    let status = crate::enums::define_status(lib)?;

    let info = lib.declare_native_struct("ErrorInfo")?;
    let info = lib
        .define_native_struct(&info)?
        .add(
            "summary",
            Type::Enum(status),
            "top level status code for the operation",
        )?
        .add(
            "exception",
            Type::Enum(exception.clone()),
            "exception code returned by the server when status == Exception",
        )?
        .add(
            "raw_exception",
            Type::Uint8,
            "raw exception code returned by the server",
        )?
        .doc("Summarizes the success or failure of an operation")?
        .build()?;

    Ok(info)
}

fn build_serial_params(lib: &mut LibraryBuilder) -> Result<NativeStructHandle, BindingError> {
    let data_bits = lib
        .define_native_enum("DataBits")?
        .push("Five", "5 bits per character")?
        .push("Six", "6 bits per character")?
        .push("Seven", "7 bits per character")?
        .push("Eight", "8 bits per character")?
        .doc("Number of bits per character")?
        .build()?;

    let flow_control = lib
        .define_native_enum("FlowControl")?
        .push("None", "No flow control")?
        .push("Software", "Flow control using XON/XOFF bytes")?
        .push("Hardware", "Flow control using RTS/CTS signals")?
        .doc("Flow control modes")?
        .build()?;

    let parity = lib
        .define_native_enum("Parity")?
        .push("None", "No parity bit")?
        .push("Odd", "Parity bit sets odd number of 1 bits")?
        .push("Even", "Parity bit sets even number of 1 bits")?
        .doc("Parity checking modes")?
        .build()?;

    let stop_bits = lib
        .define_native_enum("StopBits")?
        .push("One", "One stop bit")?
        .push("Two", "Two stop bits")?
        .doc("Number of stop bits")?
        .build()?;

    let serial_params = lib.declare_native_struct("SerialPortSettings")?;
    lib.define_native_struct(&serial_params)?
        .add(
            "baud_rate",
            StructElementType::Uint32(Some(9600)),
            "Baud rate (in symbols-per-second)",
        )?
        .add(
            "data_bits",
            StructElementType::Enum(data_bits, Some("Eight".to_string())),
            "Number of bits used to represent a character sent on the line",
        )?
        .add(
            "flow_control",
            StructElementType::Enum(flow_control, Some("None".to_string())),
            "Type of signalling to use for controlling data transfer",
        )?
        .add(
            "parity",
            StructElementType::Enum(parity, Some("None".to_string())),
            "Type of parity to use for error checking",
        )?
        .add(
            "stop_bits",
            StructElementType::Enum(stop_bits, Some("One".to_string())),
            "Number of bits to use to signal the end of a character",
        )?
        .doc("Serial port settings")?
        .build()
}

fn build_iterator(
    lib: &mut LibraryBuilder,
    value_type: &NativeStructHandle,
) -> Result<IteratorHandle, BindingError> {
    let base_name = value_type.declaration.name.clone();
    let iterator = lib.declare_class(&format!("{}Iterator", base_name))?;
    let iterator_next_fn = lib
        .declare_native_function(&format!("next_{}", base_name.to_lowercase()))?
        .param("it", Type::ClassRef(iterator), "iterator")?
        .return_type(ReturnType::new(
            Type::StructRef(value_type.declaration()),
            "next value of the iterator or NULL if the iterator has reached the end",
        ))?
        .doc("advance the iterator")?
        .build()?;

    lib.define_iterator_with_lifetime(&iterator_next_fn, value_type)
}
