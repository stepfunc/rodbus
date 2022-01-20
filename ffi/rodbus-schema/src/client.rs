use oo_bindgen::callback::InterfaceHandle;
use oo_bindgen::class::ClassDeclarationHandle;
use oo_bindgen::iterator::IteratorHandle;
use oo_bindgen::native_function::{DurationMapping, NativeFunctionHandle, ReturnType, Type};
use oo_bindgen::native_struct::{NativeStructHandle, StructElementType};
use oo_bindgen::{doc, BindingError, LibraryBuilder};

use crate::common::CommonDefinitions;
use oo_bindgen::collection::CollectionHandle;

pub(crate) fn build(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> Result<(), BindingError> {
    let channel = lib.declare_class("Channel")?;

    let retry_strategy = build_retry_strategy(lib)?;

    let create_tcp_client_fn = lib.declare_native_function("create_tcp_client")?;
    let create_tcp_client_fn = create_tcp_client_fn
        .param(
            "runtime",
            Type::ClassRef(common.runtime_handle.declaration.clone()),
            "runtime on which to create the channel",
        )?
        .param("address", Type::String, "IP address of remote host")?
        .param(
            "max_queued_requests",
            Type::Uint16,
            "Maximum number of requests to queue before failing the next request",
        )?
        .param(
            "retry_strategy",
            Type::Struct(retry_strategy),
            "Reconnection timing strategy",
        )?
        .param(
            "decode_level",
            Type::Struct(common.decode_level.clone()),
            "Decode levels for this client",
        )?
        .return_type(ReturnType::Type(
            Type::ClassRef(channel.clone()),
            "pointer to the created channel or NULL if an error occurred".into(),
        ))?
        .fails_with(common.error_type.clone())?
        .doc("create a new tcp channel instance")?
        .build()?;

    let create_rtu_client_fn = lib.declare_native_function("create_rtu_client")?;
    let create_rtu_client_fn = create_rtu_client_fn
        .param(
            "runtime",
            Type::ClassRef(common.runtime_handle.declaration.clone()),
            "runtime on which to create the channel",
        )?
        .param(
            "path",
            Type::String,
            "Path to the serial device. Generally /dev/tty0 on Linux and COM1 on Windows.",
        )?
        .param(
            "serial_params",
            Type::Struct(common.serial_port_settings.clone()),
            "Serial port settings",
        )?
        .param(
            "max_queued_requests",
            Type::Uint16,
            "Maximum number of requests to queue before failing the next request",
        )?
        .param(
            "open_retry_delay",
            Type::Duration(DurationMapping::Milliseconds),
            "Delay between attempts to open the serial port",
        )?
        .param(
            "decode_level",
            Type::Struct(common.decode_level.clone()),
            "Decode levels for this client",
        )?
        .return_type(ReturnType::Type(
            Type::ClassRef(channel.clone()),
            "pointer to the created channel or NULL if an error occurred".into(),
        ))?
        .fails_with(common.error_type.clone())?
        .doc("create a new tcp channel instance")?
        .build()?;

    let destroy_channel_fn = lib
        .declare_native_function("channel_destroy")?
        .param(
            "channel",
            Type::ClassRef(channel.clone()),
            "channel to destroy",
        )?
        .return_type(ReturnType::Void)?
        .doc("destroy a channel instance")?
        .build()?;

    let bit_read_callback = build_bit_read_callback(lib, common)?;
    let register_read_callback = build_register_read_callback(lib, common)?;
    let write_callback = build_write_callback(lib, common)?;

    let read_coils_fn = build_async_read_fn(
        "channel_read_coils",
        lib,
        common,
        &channel,
        &bit_read_callback,
        "start an asynchronous request to read coils",
    )?;

    let read_discrete_inputs_fn = build_async_read_fn(
        "channel_read_discrete_inputs",
        lib,
        common,
        &channel,
        &bit_read_callback,
        "start an asynchronous request to read discrete inputs",
    )?;

    let read_holding_registers_fn = build_async_read_fn(
        "channel_read_holding_registers",
        lib,
        common,
        &channel,
        &register_read_callback,
        "start an asynchronous request to read holding registers",
    )?;

    let read_input_registers_fn = build_async_read_fn(
        "channel_read_input_registers",
        lib,
        common,
        &channel,
        &register_read_callback,
        "start an asynchronous request to read input registers",
    )?;

    let write_single_coil_fn = build_async_write_single_fn(
        "channel_write_single_coil",
        lib,
        common,
        &channel,
        &write_callback,
        &common.bit,
        "write a single coil",
    )?;

    let write_single_register_fn = build_async_write_single_fn(
        "channel_write_single_register",
        lib,
        common,
        &channel,
        &write_callback,
        &common.register,
        "write a single register",
    )?;

    let list_of_bit = build_list(lib, "Bit", Type::Bool)?;
    let write_multiple_coils_fn = build_async_write_multiple_fn(
        "channel_write_multiple_coils",
        lib,
        common,
        &channel,
        &write_callback,
        &list_of_bit,
        "write multiple coils",
    )?;

    let list_of_register = build_list(lib, "Register", Type::Uint16)?;
    let write_multiple_registers_fn = build_async_write_multiple_fn(
        "channel_write_multiple_registers",
        lib,
        common,
        &channel,
        &write_callback,
        &list_of_register,
        "write multiple registers",
    )?;

    lib.define_class(&channel)?
        // abstract factory methods
        .static_method("create_tcp_client", &create_tcp_client_fn)?
        .static_method("create_rtu_client", &create_rtu_client_fn)?
        // read methods
        .async_method("read_coils", &read_coils_fn)?
        .async_method("read_discrete_inputs", &read_discrete_inputs_fn)?
        .async_method("read_holding_registers", &read_holding_registers_fn)?
        .async_method("read_input_registers", &read_input_registers_fn)?
        // write methods
        .async_method("write_single_coil", &write_single_coil_fn)?
        .async_method("write_single_register", &write_single_register_fn)?
        .async_method("write_multiple_coils", &write_multiple_coils_fn)?
        .async_method("write_multiple_registers", &write_multiple_registers_fn)?
        // destructor
        .destructor(&destroy_channel_fn)?
        .custom_destroy("Shutdown")?
        .doc("Abstract representation of a channel")?
        .build()?;

    Ok(())
}

fn build_async_write_single_fn(
    name: &str,
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
    channel: &ClassDeclarationHandle,
    callback: &InterfaceHandle,
    write_type: &NativeStructHandle,
    docs: &str,
) -> Result<NativeFunctionHandle, BindingError> {
    lib.declare_native_function(name)?
        .param(
            "channel",
            Type::ClassRef(channel.clone()),
            "channel on which to perform the write operation",
        )?
        .param(
            "param",
            Type::Struct(common.request_param.clone()),
            "parameters for the request",
        )?
        .param(
            "value",
            Type::Struct(write_type.clone()),
            "Address and value to write",
        )?
        .param(
            "callback",
            Type::Interface(callback.clone()),
            "callback invoked on completion",
        )?
        .return_type(ReturnType::void())?
        .fails_with(common.error_type.clone())?
        .doc(docs)?
        .build()
}

fn build_async_write_multiple_fn(
    name: &str,
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
    channel: &ClassDeclarationHandle,
    callback: &InterfaceHandle,
    list_type: &CollectionHandle,
    docs: &str,
) -> Result<NativeFunctionHandle, BindingError> {
    lib.declare_native_function(name)?
        .param(
            "channel",
            Type::ClassRef(channel.clone()),
            "channel on which to perform the write operation",
        )?
        .param(
            "param",
            Type::Struct(common.request_param.clone()),
            "parameters for the request",
        )?
        .param("start", Type::Uint16, "starting address")?
        .param(
            "items",
            Type::Collection(list_type.clone()),
            "list of items to write",
        )?
        .param(
            "callback",
            Type::Interface(callback.clone()),
            "callback invoked on completion",
        )?
        .return_type(ReturnType::void())?
        .fails_with(common.error_type.clone())?
        .doc(docs)?
        .build()
}

fn build_async_read_fn(
    name: &str,
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
    channel: &ClassDeclarationHandle,
    callback: &InterfaceHandle,
    docs: &str,
) -> Result<NativeFunctionHandle, BindingError> {
    lib.declare_native_function(name)?
        .param(
            "channel",
            Type::ClassRef(channel.clone()),
            "channel on which to perform the read",
        )?
        .param(
            "param",
            Type::Struct(common.request_param.clone()),
            "parameters for the request",
        )?
        .param(
            "range",
            Type::Struct(common.address_range.clone()),
            "range of addresses to read",
        )?
        .param(
            "callback",
            Type::Interface(callback.clone()),
            "callback invoked on completion",
        )?
        .return_type(ReturnType::void())?
        .fails_with(common.error_type.clone())?
        .doc(docs)?
        .build()
}

fn build_bit_read_callback(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> Result<InterfaceHandle, BindingError> {
    let bit_read_result =
        build_callback_struct(lib, &common.bit, &common.bit_iterator, &common.error_info)?;
    let bit_read_callback = lib
        .define_interface(
            "BitReadCallback",
            "Callback for reading coils or input registers",
        )?
        .callback(
            "on_complete",
            "Called when the operation completes or fails",
        )?
        .param("result", Type::Struct(bit_read_result), "result")?
        .return_type(ReturnType::void())?
        .build()?
        .destroy_callback("on_destroy")?
        .build()?;

    Ok(bit_read_callback)
}

fn build_register_read_callback(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> Result<InterfaceHandle, BindingError> {
    let read_result = build_callback_struct(
        lib,
        &common.register,
        &common.register_iterator,
        &common.error_info,
    )?;
    let read_callback = lib
        .define_interface(
            "RegisterReadCallback",
            "Callback for reading holding or input registers",
        )?
        .callback(
            "on_complete",
            "Called when the operation completes or fails",
        )?
        .param("result", Type::Struct(read_result), "result")?
        .return_type(ReturnType::void())?
        .build()?
        .destroy_callback("on_destroy")?
        .build()?;

    Ok(read_callback)
}

fn build_write_callback(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> Result<InterfaceHandle, BindingError> {
    lib.define_interface("WriteCallback", "Callback type for write operations")?
        .callback(
            "on_complete",
            "Called when the operation completes or fails",
        )?
        .param(
            "result",
            Type::Struct(common.error_info.clone()),
            "result of the operation",
        )?
        .return_type(ReturnType::void())?
        .build()?
        .destroy_callback("on_destroy")?
        .build()
}

fn build_callback_struct(
    lib: &mut LibraryBuilder,
    item_type: &NativeStructHandle,
    iterator_type: &IteratorHandle,
    error_info: &NativeStructHandle,
) -> Result<NativeStructHandle, BindingError> {
    let callback_struct =
        lib.declare_native_struct(format!("{}ReadResult", item_type.declaration.name).as_str())?;
    let callback_struct = lib
        .define_native_struct(&callback_struct)?
        .add(
            "result",
            Type::Struct(error_info.clone()),
            "error information",
        )?
        .add(
            "iterator",
            Type::Iterator(iterator_type.clone()),
            "iterator valid when result.summary == Ok",
        )?
        .doc("Result type returned when asynchronous operation completes or fails")?
        .build()?;

    Ok(callback_struct)
}

fn build_list(
    lib: &mut LibraryBuilder,
    name: &str,
    value_type: Type,
) -> Result<CollectionHandle, BindingError> {
    let list_class = lib.declare_class(&format!("{}List", name))?;

    let create_fn = lib
        .declare_native_function(&format!("{}_list_create", name.to_lowercase()))?
        .param(
            "size_hint",
            Type::Uint32,
            "Starting size of the list. Can be used to avoid multiple allocations if you already know how many items you're going to add.",
        )?
        .return_type(ReturnType::new(
            Type::ClassRef(list_class.clone()),
            "created list",
        ))?
        .doc(format!("create a {} list", name).as_str())?
        .build()?;

    let destroy_fn = lib
        .declare_native_function(&format!("{}_list_destroy", name.to_lowercase()))?
        .param(
            "list",
            Type::ClassRef(list_class.clone()),
            "list to destroy",
        )?
        .return_type(ReturnType::void())?
        .doc(format!("destroy a {} list", name).as_str())?
        .build()?;

    let add_fn = lib
        .declare_native_function(&format!("{}_list_add", name.to_lowercase()))?
        .param(
            "list",
            Type::ClassRef(list_class),
            "list to which to add the item",
        )?
        .param("item", value_type, "item to add to the list")?
        .return_type(ReturnType::void())?
        .doc("Add an item to the list")?
        .build()?;

    lib.define_collection(&create_fn, &destroy_fn, &add_fn)
}

fn build_retry_strategy(lib: &mut LibraryBuilder) -> Result<NativeStructHandle, BindingError> {
    let retry_strategy = lib.declare_native_struct("RetryStrategy")?;
    lib.define_native_struct(&retry_strategy)?
        .add(
            "min_delay",
            StructElementType::Duration(
                DurationMapping::Milliseconds,
                Some(std::time::Duration::from_secs(1)),
            ),
            "Minimum delay between two retries",
        )?
        .add(
            "max_delay",
            StructElementType::Duration(
                DurationMapping::Milliseconds,
                Some(std::time::Duration::from_secs(10)),
            ),
            "Maximum delay between two retries",
        )?
        .doc(doc("Retry strategy configuration.").details(
            "The strategy uses an exponential back-off with a minimum and maximum value.",
        ))?
        .build()
}
