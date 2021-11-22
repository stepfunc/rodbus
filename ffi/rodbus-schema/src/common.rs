use oo_bindgen::model::*;

pub(crate) struct CommonDefinitions {
    pub(crate) error_type: ErrorType<Unvalidated>,
    pub(crate) nothing: EnumHandle,
    pub(crate) decode_level: UniversalStructHandle,
    pub(crate) runtime_handle: ClassDeclarationHandle,
    pub(crate) error_info: ErrorType<Unvalidated>,
    pub(crate) address_range: FunctionArgStructHandle,
    pub(crate) request_param: FunctionArgStructHandle,
    pub(crate) bit_value: UniversalStructHandle,
    pub(crate) register_value: UniversalStructHandle,
    pub(crate) bit_iterator: AbstractIteratorHandle,
    pub(crate) register_iterator: AbstractIteratorHandle,
    pub(crate) exception: EnumHandle,
}

impl CommonDefinitions {
    pub(crate) fn build(lib: &mut LibraryBuilder) -> BackTraced<CommonDefinitions> {
        let error_type = build_error_type(lib)?;
        let nothing = build_nothing_type(lib)?;
        let decode_level = crate::logging::define(lib, error_type.clone())?;
        let bit_value = build_bit_value(lib)?;
        let register_value = build_register_value(lib)?;

        Ok(Self {
            error_type: error_type.clone(),
            nothing,
            decode_level,
            runtime_handle: crate::runtime::define(lib, error_type)?,
            error_info: build_request_error(lib)?,
            address_range: build_address_range(lib)?,
            request_param: build_request_param(lib)?,
            bit_value: bit_value.clone(),
            register_value: register_value.clone(),
            bit_iterator: build_iterator(lib, &bit_value)?,
            register_iterator: build_iterator(lib, &register_value)?,
            exception: build_exception(lib)?,
        })
    }
}

fn build_error_type(lib: &mut LibraryBuilder) -> BackTraced<ErrorType<Unvalidated>> {
    let definition = lib
        .define_error_type(
            "param_error",
            "param_exception",
            ExceptionType::UncheckedException,
        )?
        .add_error("null_parameter", "Null parameter")?
        .add_error(
            "logging_already_configured",
            "Logging can only be configured once",
        )?
        .add_error("runtime_creation_failure", "Failed to create tokio runtime")?
        .add_error("runtime_destroyed", "Runtime was already disposed of")?
        .add_error(
            "runtime_cannot_block_within_async",
            "Runtime cannot execute blocking call within asynchronous context",
        )?
        .add_error("invalid_socket_address", "Invalid socket address")?
        .add_error("invalid_range", "Invalid Modbus address range")?
        .add_error("invalid_request", "Invalid Modbus request")?
        .add_error("invalid_index", "Invalid index")?
        .add_error(
            "server_bind_error",
            "Server failed to bind to the specified port",
        )?
        .add_error(
            "invalid_unit_id",
            "The specified unit id is not associated to this server",
        )?
        .doc("Error type used throughout the library")?
        .build()?;

    Ok(definition)
}

fn build_nothing_type(lib: &mut LibraryBuilder) -> BackTraced<EnumHandle> {
    let definition = lib.define_enum("nothing")?
        .push("nothing", "the only value this enum has")?
        .doc("A single value enum which is used as a placeholder for futures that don't return a value")?
        .build()?;

    Ok(definition)
}

fn build_bit_value(lib: &mut LibraryBuilder) -> BackTraced<UniversalStructHandle> {
    let bit = lib.declare_universal_struct("bit_value")?;
    let bit = lib
        .define_universal_struct(bit)?
        .add("index", Primitive::U16, "Index of bit")?
        .add("value", Primitive::Bool, "Value of the bit")?
        .doc("Index/value tuple of a bit type")?
        .end_fields()?
        .add_full_initializer("init")?
        .build()?;

    Ok(bit)
}

fn build_register_value(lib: &mut LibraryBuilder) -> BackTraced<UniversalStructHandle> {
    let bit = lib.declare_universal_struct("register_value")?;
    let register = lib
        .define_universal_struct(bit)?
        .add("index", Primitive::U16, "Index of register")?
        .add("value", Primitive::U16, "Value of the register")?
        .doc("Index/value tuple of a register type")?
        .end_fields()?
        .add_full_initializer("init")?
        .build()?;

    Ok(register)
}

fn build_address_range(lib: &mut LibraryBuilder) -> BackTraced<FunctionArgStructHandle> {
    let info = lib.declare_function_arg_struct("address_range")?;
    let info = lib
        .define_function_argument_struct(info)?
        .add("start", Primitive::U16, "Starting address of the range")?
        .add("count", Primitive::U16, "Number of addresses in the range")?
        .doc("Range of 16-bit addresses")?
        .end_fields()?
        .add_full_initializer("init")?
        .build()?;

    Ok(info)
}

fn build_request_param(lib: &mut LibraryBuilder) -> BackTraced<FunctionArgStructHandle> {
    let param = lib.declare_function_arg_struct("request_param")?;
    let param = lib
        .define_function_argument_struct(param)?
        .add("unit_id", Primitive::U8, "Modbus address for the request")?
        .add(
            "timeout",
            BasicType::Duration(DurationType::Milliseconds),
            "Response timeout for the request",
        )?
        .doc("Address and timeout parameters for requests")?
        .end_fields()?
        .add_full_initializer("init")?
        .build()?;

    Ok(param)
}

fn build_iterator(
    lib: &mut LibraryBuilder,
    item_type: &UniversalStructHandle,
) -> BackTraced<AbstractIteratorHandle> {
    let base_name = item_type.declaration.name();
    let iter =
        lib.define_iterator_with_lifetime(format!("{}_iterator", base_name), item_type.clone())?;
    Ok(iter)
}

fn build_request_error(lib: &mut LibraryBuilder) -> BackTraced<ErrorType<Unvalidated>> {
    let mut builder = lib
        .define_error_type(
            "request_error",
            "request_exception",
            ExceptionType::CheckedException,
        )?
        .doc("Error information returned during asynchronous API calls")?
        .add_error(
            "shutdown",
            "The channel was shutdown before the operation could complete",
        )?
        .add_error("no_connection", "No connection could be made to the server")?
        .add_error(
            "response_timeout",
            "No valid response was received before the timeout",
        )?
        .add_error("bad_request", "The request was invalid")?
        .add_error(
            "bad_response",
            "The response from the server was received but was improperly formatted",
        )?
        .add_error(
            "io_error",
            "An I/O error occurred on the underlying stream while performing the request",
        )?
        .add_error(
            "bad_framing",
            "A framing error was detected while performing the request",
        )?
        .add_error(
            "internal_error",
            "An unspecified internal error occurred while performing the request",
        )?
        .add_error(
            "bad_argument",
            "An invalid argument was supplied and the request could not be performed",
        )?;

    for (name, _value, desc) in MODBUS_EXCEPTION {
        builder = builder.add_error(format!("modbus_exception_{}", name), desc)?;
    }

    let definition = builder.build()?;

    Ok(definition)
}

fn build_exception(lib: &mut LibraryBuilder) -> BackTraced<EnumHandle> {
    let mut builder = lib
        .define_enum("modbus_exception")?
        .doc("Error information returned during asynchronous API calls")?;

    for (name, value, desc) in MODBUS_EXCEPTION {
        builder = builder.variant(name, *value as i32, desc)?;
    }

    let definition = builder.build()?;

    Ok(definition)
}

const MODBUS_EXCEPTION: &[(&str, u8, &str)] = &[
    ("illegal_function", 0x01, "The data address received in the query is not an allowable address for the server"),
    ("illegal_data_address", 0x02, "The data address received in the query is not an allowable address for the server"),
    ("illegal_data_value", 0x03, "A value contained in the request is not an allowable value for server"),
    ("server_device_failure", 0x04, "An unrecoverable error occurred while the server was attempting to perform the requested action"),
    ("acknowledge", 0x05, "Specialized use in conjunction with  programming commands. The server has accepted the request and is processing it."),
    ("server_device_busy", 0x06, "Specialized use in conjunction with  programming commands. The server is engaged in processing a long-duration program command, try again later"),
    ("memory_parity_error", 0x08, "Specialized use in conjunction with function codes 20 and 21 and reference type 6, to indicate that the extended file area failed to pass a consistency check. The server attempted to read a record file, but detected a parity error in the memory"),
    ("gateway_path_unavailable", 0x0A, "Specialized use in conjunction with gateways, indicates that the gateway was unable to allocate an internal communication path from the input port to the output port for processing the request. Usually means that the gateway is mis-configured or overloaded"),
    ("gateway_target_device_failed_to_respond", 0x0B, "Specialized use in conjunction with gateways, indicates that no response was obtained from the target device. Usually means that the device is not present on the network"),
    ("unknown", 0xFF, "The status code is not defined in the Modbus specification, refer to the raw exception code to see what the server sent"),
];
