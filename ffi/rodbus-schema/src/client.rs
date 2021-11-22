use oo_bindgen::model::*;

use crate::common::CommonDefinitions;

pub(crate) fn build(lib: &mut LibraryBuilder, common: &CommonDefinitions) -> BackTraced<()> {
    let channel = lib.declare_class("channel")?;

    let retry_strategy = build_retry_strategy(lib)?;

    let create_tcp_client_fn = lib
        .define_function("create_tcp_client")?
        .param(
            "runtime",
            common.runtime_handle.clone(),
            "Runtime on which to create the channel",
        )?
        .param("address", StringType, "IP address of remote host")?
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
        .doc("Create a new tcp channel instance")?
        .build_static_with_same_name()?;

    let destroy_channel_fn = lib.define_destructor(
        channel.clone(),
        "Shutdown a {class:channel} and release all resources",
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

    lib.define_class(&channel)?
        // abstract factory methods, later we'll have TLS/serial
        .static_method(create_tcp_client_fn)?
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
    callback: FutureInterface<Unvalidated>,
    docs: &str,
) -> BackTraced<FutureMethod<Unvalidated>> {
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
    callback: FutureInterface<Unvalidated>,
    write_type: UniversalStructHandle,
    docs: &str,
) -> BackTraced<FutureMethod<Unvalidated>> {
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
    callback: FutureInterface<Unvalidated>,
    list_type: CollectionHandle,
    docs: &str,
) -> BackTraced<FutureMethod<Unvalidated>> {
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
) -> BackTraced<FutureInterface<Unvalidated>> {
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
) -> BackTraced<FutureInterface<Unvalidated>> {
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
) -> BackTraced<FutureInterface<Unvalidated>> {
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
            BasicType::Duration(DurationType::Milliseconds),
            "Minimum delay between two retries",
        )?
        .add(
            &max_delay_field,
            BasicType::Duration(DurationType::Milliseconds),
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
