use oo_bindgen::model::*;

use crate::common::CommonDefinitions;

pub(crate) fn build(lib: &mut LibraryBuilder, common: &CommonDefinitions) -> BackTraced<()> {
    let channel = lib.declare_class("client_channel")?;

    let retry_strategy = build_retry_strategy(lib)?;
    let tls_client_config = build_tls_client_config(lib, common)?;

    let tcp_client_create_fn = lib
        .define_function("client_channel_create_tcp")?
        .param(
            "runtime",
            common.runtime_handle.clone(),
            "Runtime on which to create the channel",
        )?
        .param(
            "host",
            StringType,
            "IP (v4/v6) or host name remote endpoint",
        )?
        .param("port", Primitive::U16, "remote port")?
        .param(
            "max_queued_requests",
            Primitive::U16,
            "Maximum number of requests to queue before failing the next request",
        )?
        .param(
            "retry_strategy",
            retry_strategy.clone(),
            "Reconnection timing strategy",
        )?
        .param(
            "decode_level",
            common.decode_level.clone(),
            "Decode levels for this client",
        )?
        .returns(channel.clone(), "Pointer to the created channel")?
        .fails_with(common.error_type.clone())?
        .doc("Create a new TCP channel instance")?
        .build_static("create_tcp")?;

    let rtu_client_create_fn = lib
        .define_function("client_channel_create_rtu")?
        .param(
            "runtime",
            common.runtime_handle.clone(),
            "runtime on which to create the channel",
        )?
        .param(
            "path",
            StringType,
            "Path to the serial device. Generally /dev/tty0 on Linux and COM1 on Windows.",
        )?
        .param(
            "serial_params",
            common.serial_port_settings.clone(),
            "Serial port settings",
        )?
        .param(
            "max_queued_requests",
            Primitive::U16,
            "Maximum number of requests to queue before failing the next request",
        )?
        .param(
            "open_retry_delay",
            DurationType::Milliseconds,
            "Delay between attempts to open the serial port",
        )?
        .param(
            "decode_level",
            common.decode_level.clone(),
            "Decode levels for this client",
        )?
        .returns(channel.clone(), "Pointer to the created channel")?
        .fails_with(common.error_type.clone())?
        .doc("Create a new RTU channel instance")?
        .build_static("create_rtu")?;

    let tls_client_create_fn = lib
        .define_function("client_channel_create_tls")?
        .param(
            "runtime",
            common.runtime_handle.clone(),
            "Runtime on which to create the channel",
        )?
        .param(
            "host",
            StringType,
            "IP (v4/v6) or host name remote endpoint",
        )?
        .param("port", Primitive::U16, "remote port")?
        .param(
            "max_queued_requests",
            Primitive::U16,
            "Maximum number of requests to queue before failing the next request",
        )?
        .param(
            "retry_strategy",
            retry_strategy,
            "Reconnection timing strategy",
        )?
        .param("tls_config", tls_client_config, "TLS client configuration")?
        .param(
            "decode_level",
            common.decode_level.clone(),
            "Decode levels for this client",
        )?
        .returns(
            channel.clone(),
            "Pointer to the created channel or {null} if an error occurred",
        )?
        .fails_with(common.error_type.clone())?
        .doc("Create a new TLS channel instance")?
        .build_static("create_tls")?;

    let destroy_channel_fn = lib.define_destructor(
        channel.clone(),
        "Shutdown a {class:client_channel} and release all resources",
    )?;

    let bit_read_callback = build_bit_read_callback(lib, common)?;
    let register_read_callback = build_register_read_callback(lib, common)?;
    let write_callback = build_write_callback(lib, common)?;

    let read_coils_method = build_async_read_method(
        "read_coils",
        lib,
        common,
        channel.clone(),
        bit_read_callback.clone(),
        "Start an asynchronous request to read coils",
    )?;

    let read_discrete_inputs_method = build_async_read_method(
        "read_discrete_inputs",
        lib,
        common,
        channel.clone(),
        bit_read_callback,
        "Start an asynchronous request to read discrete inputs",
    )?;

    let read_holding_registers_method = build_async_read_method(
        "read_holding_registers",
        lib,
        common,
        channel.clone(),
        register_read_callback.clone(),
        "Start an asynchronous request to read holding registers",
    )?;

    let read_input_registers_method = build_async_read_method(
        "read_input_registers",
        lib,
        common,
        channel.clone(),
        register_read_callback,
        "Start an asynchronous request to read input registers",
    )?;

    let write_single_coil_method = build_async_write_single_method(
        "write_single_coil",
        lib,
        common,
        channel.clone(),
        write_callback.clone(),
        common.bit_value.clone(),
        "Write a single coil",
    )?;

    let write_single_register_method = build_async_write_single_method(
        "write_single_register",
        lib,
        common,
        channel.clone(),
        write_callback.clone(),
        common.register_value.clone(),
        "Write a single register",
    )?;

    let list_of_bits = lib.define_collection("bit_list", Primitive::Bool, true)?;
    let write_multiple_coils_method = build_async_write_multiple_method(
        "write_multiple_coils",
        lib,
        common,
        channel.clone(),
        write_callback.clone(),
        list_of_bits,
        "Write multiple coils",
    )?;

    let list_of_registers = lib.define_collection("register_list", Primitive::U16, true)?;
    let write_multiple_registers_method = build_async_write_multiple_method(
        "write_multiple_registers",
        lib,
        common,
        channel.clone(),
        write_callback,
        list_of_registers,
        "Write multiple registers",
    )?;

    let set_decode_level_fn = lib
        .define_method("set_decode_level", channel.clone())?
        .param("level", common.decode_level.clone(), "Decoding level")?
        .fails_with(common.error_type.clone())?
        .doc("Set the decoding level for the channel")?
        .build()?;

    lib.define_class(&channel)?
        // abstract factory methods
        .static_method(tcp_client_create_fn)?
        .static_method(rtu_client_create_fn)?
        .static_method(tls_client_create_fn)?
        // setting methods
        .method(set_decode_level_fn)?
        // read methods
        .async_method(read_coils_method)?
        .async_method(read_discrete_inputs_method)?
        .async_method(read_holding_registers_method)?
        .async_method(read_input_registers_method)?
        // write methods
        .async_method(write_single_coil_method)?
        .async_method(write_single_register_method)?
        .async_method(write_multiple_coils_method)?
        .async_method(write_multiple_registers_method)?
        // destructor
        .destructor(destroy_channel_fn)?
        .custom_destroy("shutdown")? // custom name of the destructor
        .doc("Abstract representation of a channel")?
        .build()?;

    Ok(())
}

fn build_async_read_method(
    name: &str,
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
    channel: ClassDeclarationHandle,
    callback: FutureInterfaceHandle,
    docs: &str,
) -> BackTraced<FutureMethodHandle> {
    let method = lib
        .define_future_method(name, channel, callback)?
        .param(
            "param",
            common.request_param.clone(),
            "Parameters for the request",
        )?
        .param(
            "range",
            common.address_range.clone(),
            "Range of addresses to read",
        )?
        .fails_with(common.error_type.clone())?
        .doc(docs)?
        .build()?;

    Ok(method)
}

fn build_async_write_single_method(
    name: &str,
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
    channel: ClassDeclarationHandle,
    callback: FutureInterfaceHandle,
    write_type: UniversalStructHandle,
    docs: &str,
) -> BackTraced<FutureMethodHandle> {
    let method = lib
        .define_future_method(name, channel, callback)?
        .param(
            "param",
            common.request_param.clone(),
            "Parameters for the request",
        )?
        .param("value", write_type, "Address and value to write")?
        .fails_with(common.error_type.clone())?
        .doc(docs)?
        .build()?;

    Ok(method)
}

fn build_async_write_multiple_method(
    name: &str,
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
    channel: ClassDeclarationHandle,
    callback: FutureInterfaceHandle,
    list_type: CollectionHandle,
    docs: &str,
) -> BackTraced<FutureMethodHandle> {
    let method = lib
        .define_future_method(name, channel, callback)?
        .param(
            "param",
            common.request_param.clone(),
            "Parameters for the request",
        )?
        .param("start", Primitive::U16, "Starting address")?
        .param("items", list_type, "List of items to write")?
        .fails_with(common.error_type.clone())?
        .doc(docs)?
        .build()?;

    Ok(method)
}

fn build_bit_read_callback(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> BackTraced<FutureInterfaceHandle> {
    let future = lib.define_future_interface(
        "bit_read_callback",
        "Callback for reading coils or discrete inputs",
        common.bit_iterator.clone(),
        "response",
        Some(common.error_info.clone()),
    )?;

    Ok(future)
}

fn build_register_read_callback(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> BackTraced<FutureInterfaceHandle> {
    let future = lib.define_future_interface(
        "register_read_callback",
        "Callback for reading holding or input registers",
        common.register_iterator.clone(),
        "response",
        Some(common.error_info.clone()),
    )?;

    Ok(future)
}

fn build_write_callback(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> BackTraced<FutureInterfaceHandle> {
    let future = lib.define_future_interface(
        "write_callback",
        "Callback for write operations",
        common.nothing.clone(),
        "response",
        Some(common.error_info.clone()),
    )?;

    Ok(future)
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

fn build_tls_client_config(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> BackTraced<FunctionArgStructHandle> {
    let min_tls_version_field = Name::create("min_tls_version")?;
    let certificate_mode_field = Name::create("certificate_mode")?;

    let tls_client_config = lib.declare_function_argument_struct("tls_client_config")?;
    let tls_client_config = lib.define_function_argument_struct(tls_client_config)?
        .add("dns_name", StringType, "Expected name to validate in the presented certificate (only in {enum:certificate_mode.authority_based} mode)")?
        .add(
            "peer_cert_path",
            StringType,
            "Path to the PEM-encoded certificate of the peer",
        )?
        .add(
            "local_cert_path",
            StringType,
            "Path to the PEM-encoded local certificate",
        )?
        .add(
            "private_key_path",
            StringType,
            "Path to the the PEM-encoded private key",
        )?
        .add(
            "password",
            StringType,
            doc("Optional password if the private key file is encrypted").details("Only PKCS#8 encrypted files are supported.").details("Pass empty string if the file is not encrypted.")
        )?
        .add(
            &min_tls_version_field,
            common.min_tls_version.clone(),
            "Minimum TLS version allowed",
        )?
        .add(&certificate_mode_field, common.certificate_mode.clone(), "Certficate validation mode")?
        .doc("TLS client configuration")?
        .end_fields()?
        .begin_initializer("init", InitializerType::Normal, "Initialize a TLS client configuration")?
        .default_variant(&min_tls_version_field, "v12")?
        .default_variant(&certificate_mode_field, "authority_based")?
        .end_initializer()?
        .build()?;

    Ok(tls_client_config)
}
