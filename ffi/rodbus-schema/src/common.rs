use oo_bindgen::model::*;

pub(crate) struct CommonDefinitions {
    pub(crate) error_type: ErrorTypeHandle,
    pub(crate) nothing: EnumHandle,
    pub(crate) decode_level: UniversalStructHandle,
    pub(crate) runtime_handle: ClassDeclarationHandle,
    pub(crate) error_info: ErrorTypeHandle,
    pub(crate) address_range: UniversalStructHandle,
    pub(crate) request_param: FunctionArgStructHandle,
    pub(crate) bit_value: UniversalStructHandle,
    pub(crate) register_value: UniversalStructHandle,
    pub(crate) bit_iterator: AbstractIteratorHandle,
    pub(crate) register_iterator: AbstractIteratorHandle,
    pub(crate) exception: EnumHandle,
    pub(crate) serial_port_settings: FunctionArgStructHandle,
    pub(crate) min_tls_version: EnumHandle,
    pub(crate) certificate_mode: EnumHandle,
    pub(crate) retry_strategy: UniversalStructHandle,
}

impl CommonDefinitions {
    pub(crate) fn build(lib: &mut LibraryBuilder) -> BackTraced<CommonDefinitions> {
        let error_type = build_error_type(lib)?;
        let nothing = build_nothing_type(lib)?;
        let decode_level = crate::decoding::define(lib)?;
        let bit_value = build_bit_value(lib)?;
        let register_value = build_register_value(lib)?;

        Ok(Self {
            error_type: error_type.clone(),
            nothing,
            decode_level,
            runtime_handle: sfio_tokio_ffi::define(lib, error_type)?,
            error_info: build_request_error(lib)?,
            address_range: build_address_range(lib)?,
            request_param: build_request_param(lib)?,
            bit_value: bit_value.clone(),
            register_value: register_value.clone(),
            bit_iterator: build_iterator(lib, &bit_value)?,
            register_iterator: build_iterator(lib, &register_value)?,
            exception: build_exception(lib)?,
            serial_port_settings: build_serial_params(lib)?,
            min_tls_version: build_min_tls_version(lib)?,
            certificate_mode: build_certificate_mode(lib)?,
            retry_strategy: build_retry_strategy(lib)?,
        })
    }
}

fn build_retry_strategy(lib: &mut LibraryBuilder) -> BackTraced<UniversalStructHandle> {
    let min_delay_field = Name::create("min_delay")?;
    let max_delay_field = Name::create("max_delay")?;

    let retry_strategy = lib.declare_universal_struct("retry_strategy")?;
    let retry_strategy = lib
        .define_universal_struct(retry_strategy)?
        .add(
            &min_delay_field,
            DurationType::Milliseconds,
            "Minimum delay between two retries",
        )?
        .add(
            &max_delay_field,
            DurationType::Milliseconds,
            "Maximum delay between two retries",
        )?
        .doc(doc("Retry strategy configuration.").details(
            "The strategy uses an exponential back-off with a minimum and maximum value.",
        ))?
        .end_fields()?
        .begin_initializer(
            "init",
            InitializerType::Normal,
            "Initialize a retry strategy to defaults",
        )?
        .default(&min_delay_field, std::time::Duration::from_secs(1))?
        .default(&max_delay_field, std::time::Duration::from_secs(10))?
        .end_initializer()?
        .build()?;

    Ok(retry_strategy)
}

fn build_error_type(lib: &mut LibraryBuilder) -> BackTraced<ErrorTypeHandle> {
    let definition = lib
        .define_error_type(
            "param_error",
            "param_exception",
            ExceptionType::UncheckedException,
        )?
        .add_error(
            "no_support",
            "The FFI library was compiled without support for this feature",
        )?
        .add_error("null_parameter", "Null parameter")?
        .add_error(
            "logging_already_configured",
            "Logging can only be configured once",
        )?
        .add_error("runtime_creation_failure", "Failed to create Tokio runtime")?
        .add_error("runtime_destroyed", "Runtime was already disposed of")?
        .add_error(
            "runtime_cannot_block_within_async",
            "Runtime cannot execute blocking call within asynchronous context",
        )?
        .add_error("invalid_ip_address", "Invalid IP address")?
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
        .add_error("invalid_peer_certificate", "Invalid peer certificate file")?
        .add_error(
            "invalid_local_certificate",
            "Invalid local certificate file",
        )?
        .add_error("invalid_private_key", "Invalid private key file")?
        .add_error("invalid_dns_name", "Invalid DNS name")?
        .add_error("bad_tls_config", "Bad TLS configuration")?
        .add_error("shutdown", "The task has been shutdown")?
        .add_error("invalid_utf8", "String argument was not valid UTF-8")?
        .add_error(
            "too_many_requests",
            "Number of requests exceeds configured limit",
        )?
        .doc("Error type that indicates a bad parameter or bad programmer logic")?
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

fn build_address_range(lib: &mut LibraryBuilder) -> BackTraced<UniversalStructHandle> {
    let info = lib.declare_universal_struct("address_range")?;
    let info = lib
        .define_universal_struct(info)?
        .add("start", Primitive::U16, "Starting address of the range")?
        .add("count", Primitive::U16, "Number of addresses in the range")?
        .doc("Range of 16-bit addresses sent in a request from the client to the server")?
        .end_fields()?
        .add_full_initializer("init")?
        .build()?;

    Ok(info)
}

fn build_request_param(lib: &mut LibraryBuilder) -> BackTraced<FunctionArgStructHandle> {
    let param = lib.declare_function_argument_struct("request_param")?;
    let param = lib
        .define_function_argument_struct(param)?
        .add("unit_id", Primitive::U8, "Modbus address for the request")?
        .add(
            "timeout",
            DurationType::Milliseconds,
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
    let base_name = item_type.name();
    let iter =
        lib.define_iterator_with_lifetime(format!("{base_name}_iterator"), item_type.clone())?;
    Ok(iter)
}

fn build_request_error(lib: &mut LibraryBuilder) -> BackTraced<ErrorTypeHandle> {
    let mut builder = lib
        .define_error_type(
            "request_error",
            "request_exception",
            ExceptionType::CheckedException,
        )?
        .doc(
            doc("Error information returned from asynchronous functions calls.")
                .details("Unlike {enum:param_error}, the values here generally represent spontaneous failures that are outside developer control, e.g. network failures, etc")
        )?
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
        builder = builder.add_error(format!("modbus_exception_{name}"), desc)?;
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

fn build_min_tls_version(lib: &mut LibraryBuilder) -> BackTraced<EnumHandle> {
    let definition = lib
        .define_enum("min_tls_version")?
        .push("v12", "TLS 1.2")?
        .push("v13", "TLS 1.3")?
        .doc("Minimum TLS version to allow")?
        .build()?;

    Ok(definition)
}

fn build_certificate_mode(lib: &mut LibraryBuilder) -> BackTraced<EnumHandle> {
    let definition = lib.define_enum("certificate_mode")?
        .push("authority_based",
        doc("Validates the peer certificate against one or more configured trust anchors")
            .details("This mode uses the default certificate verifier in `rustls` to ensure that the chain of certificates presented by the peer is valid against one of the configured trust anchors.")
            .details("The name verification is relaxed to allow for certificates that do not contain the SAN extension. In these cases the name is verified using the Common Name instead.")
        )?
        .push("self_signed",
            doc("Validates that the peer presents a single certificate which is a byte-for-byte match against the configured peer certificate")
                .details("The certificate is parsed only to ensure that the `NotBefore` and `NotAfter` are valid for the current system time.")
        )?
        .doc(
            doc("Determines how the certificate(s) presented by the peer are validated")
                .details("This validation always occurs **after** the handshake signature has been verified."))?
        .build()?;

    Ok(definition)
}

fn build_serial_params(lib: &mut LibraryBuilder) -> BackTraced<FunctionArgStructHandle> {
    let data_bits = lib
        .define_enum("data_bits")?
        .push("five", "5 bits per character")?
        .push("six", "6 bits per character")?
        .push("seven", "7 bits per character")?
        .push("eight", "8 bits per character")?
        .doc("Number of bits per character")?
        .build()?;

    let flow_control = lib
        .define_enum("flow_control")?
        .push("none", "No flow control")?
        .push("software", "Flow control using XON/XOFF bytes")?
        .push("hardware", "Flow control using RTS/CTS signals")?
        .doc("Flow control modes")?
        .build()?;

    let parity = lib
        .define_enum("parity")?
        .push("none", "No parity bit")?
        .push("odd", "Parity bit sets odd number of 1 bits")?
        .push("even", "Parity bit sets even number of 1 bits")?
        .doc("Parity checking modes")?
        .build()?;

    let stop_bits = lib
        .define_enum("stop_bits")?
        .push("one", "One stop bit")?
        .push("two", "Two stop bits")?
        .doc("Number of stop bits")?
        .build()?;

    let baud_rate_field = Name::create("baud_rate")?;
    let data_bits_field = Name::create("data_bits")?;
    let flow_control_field = Name::create("flow_control")?;
    let parity_field = Name::create("parity")?;
    let stop_bits_field = Name::create("stop_bits")?;

    let serial_params = lib.declare_function_argument_struct("serial_port_settings")?;
    let serial_params = lib
        .define_function_argument_struct(serial_params)?
        .add(
            &baud_rate_field,
            Primitive::U32,
            "Baud rate (in symbols-per-second)",
        )?
        .add(
            &data_bits_field,
            data_bits,
            "Number of bits used to represent a character sent on the line",
        )?
        .add(
            &flow_control_field,
            flow_control,
            "Type of signalling to use for controlling data transfer",
        )?
        .add(
            &parity_field,
            parity,
            "Type of parity to use for error checking",
        )?
        .add(
            &stop_bits_field,
            stop_bits,
            "Number of bits to use to signal the end of a character",
        )?
        .doc("Serial port settings")?
        .end_fields()?
        .begin_initializer(
            "init",
            InitializerType::Normal,
            "Initialize a serial port configuration",
        )?
        .default(&baud_rate_field, NumberValue::U32(9600))?
        .default_variant(&data_bits_field, "eight")?
        .default_variant(&flow_control_field, "none")?
        .default_variant(&parity_field, "none")?
        .default_variant(&stop_bits_field, "one")?
        .end_initializer()?
        .build()?;

    Ok(serial_params)
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
