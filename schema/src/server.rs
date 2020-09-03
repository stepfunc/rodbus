use crate::common::CommonDefinitions;

use oo_bindgen::callback::InterfaceHandle;
use oo_bindgen::class::ClassHandle;
use oo_bindgen::native_function::{ReturnType, Type};
use oo_bindgen::{BindingError, LibraryBuilder};

pub(crate) fn build(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> Result<(), BindingError> {
    let _write_handler = build_write_handler_interface(lib, common)?;
    let _handler_map = build_handler_map(lib, common)?;
    Ok(())
}

pub(crate) fn build_handler_map(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> Result<ClassHandle, BindingError> {
    let device_map = lib.declare_class("DeviceMap")?;

    let create_map = lib
        .declare_native_function("create_device_map")?
        .param(
            "runtime",
            Type::ClassRef(common.runtime_handle.declaration.clone()),
            "Runtime on which the device will be bound to a server",
        )?
        .return_type(ReturnType::Type(
            Type::ClassRef(device_map.clone()),
            "Device map instance".into(),
        ))?
        .doc("Create a device map that will be used to bind devices to a server endpoint")?
        .build()?;

    let destroy_map = lib
        .declare_native_function("destroy_device_map")?
        .param(
            "map",
            Type::ClassRef(device_map.clone()),
            "value to destroy",
        )?
        .return_type(ReturnType::void())?
        .doc("Destroy a previously created device map")?
        .build()?;

    lib.define_class(&device_map)?
        .constructor(&create_map)?
        .destructor(&destroy_map)?
        .doc("Maps endpoint handlers to Modbus address")?
        .build()
}

pub(crate) fn build_write_handler_interface(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> Result<InterfaceHandle, BindingError> {
    lib.define_interface(
        "WriteHandler",
        "Interface used to handle write requests received from the client",
    )?
    // --- write single coil ---
    .callback(
        "write_single_coil",
        "write a single coil received from the client",
    )?
    .param("value", Type::Bool, "Value of the coil to write")?
    .param("index", Type::Uint16, "Index of the coil")?
    .return_type(ReturnType::Type(
        Type::Bool,
        "true if the value exists and was written, false otherwise".into(),
    ))?
    .build()?
    // --- write single register ---
    .callback(
        "write_single_register",
        "write a single coil received from the client",
    )?
    .param("value", Type::Uint16, "Value of the register to write")?
    .param("index", Type::Uint16, "Index of the register")?
    .return_type(ReturnType::Type(
        Type::Bool,
        "true if the value exists and was written, false otherwise".into(),
    ))?
    .build()?
    // --- write multiple coils ---
    .callback(
        "write_multiple_coils",
        "write multiple coils received from the client",
    )?
    .param("start", Type::Uint16, "starting address")?
    .param(
        "it",
        Type::Iterator(common.bit_iterator.clone()),
        "iterator over coil values",
    )?
    .return_type(ReturnType::Type(
        Type::Bool,
        "true if the values exist and were written, false otherwise".into(),
    ))?
    .build()?
    // --- write multiple registers ---
    .callback(
        "write_multiple_registers",
        "write multiple registers received from the client",
    )?
    .param("start", Type::Uint16, "starting address")?
    .param(
        "it",
        Type::Iterator(common.register_iterator.clone()),
        "iterator over register values",
    )?
    .return_type(ReturnType::Type(
        Type::Bool,
        "true if the values exist and were written, false otherwise".into(),
    ))?
    .build()?
    // -------------------------------
    .destroy_callback("destroy")?
    .build()
}
