use oo_bindgen::class::ClassHandle;
use oo_bindgen::iterator::IteratorHandle;
use oo_bindgen::native_function::{ReturnType, Type};
use oo_bindgen::native_struct::NativeStructHandle;
use oo_bindgen::{BindingError, LibraryBuilder};

use crate::common::CommonDefinitions;

pub(crate) fn build_channel_class(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> Result<ClassHandle, BindingError> {
    let channel = lib.declare_class("Channel")?;

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
        .return_type(ReturnType::Type(
            Type::ClassRef(channel.clone()),
            "pointer to the created channel or NULL if an error occurred".into(),
        ))?
        .doc("create a new tcp channel instance")?
        .build()?;

    let destroy_channel_fn = lib.declare_native_function("destroy_channel")?;

    let destroy_channel_fn = destroy_channel_fn
        .param(
            "channel",
            Type::ClassRef(channel.clone()),
            "channel to destroy",
        )?
        .return_type(ReturnType::Void)?
        .doc("destroy a channel instance")?
        .build()?;

    let read_coils_result =
        build_callback_struct("Bit", common.error_info.clone(), Type::Bool, lib)?;

    let read_coils_cb = lib
        .define_one_time_callback("ReadCoilsCallback", "Callback for reading coils")?
        .callback(
            "on_complete",
            "Called when the operation is complete or fails",
        )?
        .param("result", Type::Struct(read_coils_result), "result")?
        .arg("ctx")?
        .return_type(ReturnType::void())?
        .build()?
        .arg("ctx")?
        .build()?;

    let read_coils_fn = lib
        .declare_native_function("channel_read_coils_async")?
        .param(
            "channel",
            Type::ClassRef(channel.clone()),
            "channel on which to perform the read",
        )?
        .param(
            "range",
            Type::Struct(common.address_range.clone()),
            "range of addresses to read",
        )?
        .param(
            "callback",
            Type::OneTimeCallback(read_coils_cb),
            "callback invoked on completion",
        )?
        .return_type(ReturnType::void())?
        .doc("start an asynchronous request to read coils")?
        .build()?;

    let channel = lib
        .define_class(&channel)?
        .static_method("create_tcp_client", &create_tcp_client_fn)?
        .async_method("read_coils", &read_coils_fn)?
        .destructor(&destroy_channel_fn)?
        .doc("Abstract representation of a channel")?
        .build()?;

    Ok(channel)
}

fn build_callback_struct(
    name: &str,
    error_info: NativeStructHandle,
    value_type: Type,
    lib: &mut LibraryBuilder,
) -> Result<NativeStructHandle, BindingError> {
    let iter = build_point_iterator(name, value_type, lib)?;
    let callback_struct = lib.declare_native_struct(format!("{}Result", name).as_str())?;
    let callback_struct = lib
        .define_native_struct(&callback_struct)?
        .add("result", Type::Struct(error_info), "error information")?
        .add(
            "iterator",
            Type::Iterator(iter),
            "iterator valid when result.summary == Ok",
        )?
        .doc("Result type returned when asynchronous operation completes or fails")?
        .build()?;

    Ok(callback_struct)
}

fn build_point_iterator(
    name: &str,
    value_type: Type,
    lib: &mut LibraryBuilder,
) -> Result<IteratorHandle, BindingError> {
    let item_struct = lib.declare_native_struct(name)?;
    let item_struct = lib
        .define_native_struct(&item_struct)?
        .add("index", Type::Uint16, "index of point")?
        .add("value", value_type, "value of point")?
        .doc(format!("index/value tuple for iterating over {} type", name).as_str())?
        .build()?;

    let iterator = lib.declare_class(&format!("{}Iterator", name))?;
    let iterator_next_fn = lib
        .declare_native_function(&format!("next_{}", name.to_lowercase()))?
        .param("it", Type::ClassRef(iterator), "iterator")?
        .return_type(ReturnType::new(
            Type::StructRef(item_struct.declaration()),
            "next value of the iterator or NULL if the iterator has reached the end",
        ))?
        .doc("advance the iterator")?
        .build()?;

    let item_iterator = lib.define_iterator_with_lifetime(&iterator_next_fn, &item_struct)?;

    Ok(item_iterator)
}
