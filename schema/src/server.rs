use crate::common::CommonDefinitions;

use oo_bindgen::callback::InterfaceHandle;
use oo_bindgen::class::ClassHandle;
use oo_bindgen::native_function::{ReturnType, Type};
use oo_bindgen::native_struct::NativeStructHandle;
use oo_bindgen::{BindingError, LibraryBuilder};

pub(crate) fn build(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> Result<(), BindingError> {
    let _server = build_server(lib, common)?;
    Ok(())
}

pub(crate) fn build_server(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> Result<ClassHandle, BindingError> {
    let handler_map = build_handler_map(lib, common)?;

    let server_handle = lib.declare_class("ServerHandle")?;

    let create_fn = lib
        .declare_native_function("create_tcp_server")?
        .param(
            "runtime",
            Type::ClassRef(common.runtime_handle.declaration.clone()),
            "runtime on which to spawn the server",
        )?
        .param("address", Type::String, "IPv4 or IPv6 host/port string")?
        .param(
            "endpoints",
            Type::ClassRef(handler_map.declaration.clone()),
            "map of endpoints, destroyed upon passing to this function",
        )?
        .return_type(ReturnType::Type(
            Type::ClassRef(server_handle.clone()),
            "handle to the server".into(),
        ))?
        .doc("Launch a TCP server to handle")?
        .build()?;

    let destroy_fn = lib
        .declare_native_function("destroy_server")?
        .param(
            "server",
            Type::ClassRef(server_handle.clone()),
            "handle of the server to destroy",
        )?
        .return_type(ReturnType::void())?
        .doc("destroy a running server via its handle")?
        .build()?;

    lib.define_class(&server_handle)?
        .constructor(&create_fn)?
        .destructor(&destroy_fn)?
        .doc("Server handle, the server remains alive until this reference is destroyed")?
        .build()
}

pub(crate) fn build_handler_map(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> Result<ClassHandle, BindingError> {
    let write_handler = build_write_handler_interface(lib, common)?;

    let device_map = lib.declare_class("DeviceMap")?;

    let create_map = lib
        .declare_native_function("create_device_map")?
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

    let sizes = build_sizes_struct(lib)?;

    let map_add_endpoint = lib
        .declare_native_function("map_add_endpoint")?
        .param(
            "map",
            Type::ClassRef(device_map.clone()),
            "map to which the endpoint will be added",
        )?
        .param("unit_id", Type::Uint8, "Unit id of the endpoint")?
        .param(
            "sizes",
            Type::Struct(sizes),
            "number of points of each type",
        )?
        .param(
            "handler",
            Type::Interface(write_handler),
            "callback interface for handling write operations for this device",
        )?
        .return_type(ReturnType::Type(
            Type::Bool,
            "false if the unit id is already bound, true otherwise".into(),
        ))?
        .doc("add an endpoint to the map")?
        .build()?;

    lib.define_class(&device_map)?
        .constructor(&create_map)?
        .destructor(&destroy_map)?
        .method("add_endpoint", &map_add_endpoint)?
        .doc("Maps endpoint handlers to Modbus address")?
        .build()
}

pub(crate) fn build_sizes_struct(
    lib: &mut LibraryBuilder,
) -> Result<NativeStructHandle, BindingError> {
    let sizes = lib.declare_native_struct("Sizes")?;

    lib.define_native_struct(&sizes)?
        .add("num_coils", Type::Uint16, "number of coils")?
        .add(
            "num_discrete_inputs",
            Type::Uint16,
            "number of discrete inputs",
        )?
        .add(
            "num_holding_registers",
            Type::Uint16,
            "number of holding registers",
        )?
        .add(
            "num_input_registers",
            Type::Uint16,
            "number of input registers",
        )?
        .doc("Number of points of each type")?
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
