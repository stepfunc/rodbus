use crate::common::CommonDefinitions;

use oo_bindgen::callback::InterfaceHandle;
use oo_bindgen::class::ClassHandle;
use oo_bindgen::native_function::{ReturnType, Type};
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
            "map of endpoints which is emptied upon passing to this function",
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

pub(crate) fn build_database_class(lib: &mut LibraryBuilder) -> Result<ClassHandle, BindingError> {
    let database = lib.declare_class("Database")?;

    let add_coil_fn = lib
        .declare_native_function("database_add_coil")?
        .param(
            "database",
            Type::ClassRef(database.clone()),
            "database to manipulate",
        )?
        .param("index", Type::Uint16, "address of the coil")?
        .param("value", Type::Bool, "initial value of the coil")?
        .return_type(ReturnType::Type(
            Type::Bool,
            "true if the value is new, false otherwise".into(),
        ))?
        .doc("add a new coil to the database")?
        .build()?;

    let add_discrete_input_fn = lib
        .declare_native_function("database_add_discrete_input")?
        .param(
            "database",
            Type::ClassRef(database.clone()),
            "database to manipulate",
        )?
        .param("index", Type::Uint16, "address of the point")?
        .param("value", Type::Bool, "initial value of the point")?
        .return_type(ReturnType::Type(
            Type::Bool,
            "true if the value is new, false otherwise".into(),
        ))?
        .doc("add a new discrete input to the database")?
        .build()?;

    let add_holding_register_fn = lib
        .declare_native_function("database_add_holding_register")?
        .param(
            "database",
            Type::ClassRef(database.clone()),
            "database to manipulate",
        )?
        .param("index", Type::Uint16, "address of the holding register")?
        .param(
            "value",
            Type::Uint16,
            "initial value of the holding register",
        )?
        .return_type(ReturnType::Type(
            Type::Bool,
            "true if the value is new, false otherwise".into(),
        ))?
        .doc("add a new holding register to the database")?
        .build()?;

    let add_input_register_fn = lib
        .declare_native_function("database_add_input_register")?
        .param(
            "database",
            Type::ClassRef(database.clone()),
            "database to manipulate",
        )?
        .param("index", Type::Uint16, "address of the input register")?
        .param("value", Type::Uint16, "initial value of the input register")?
        .return_type(ReturnType::Type(
            Type::Bool,
            "true if the value is new, false otherwise".into(),
        ))?
        .doc("add a new input register to the database")?
        .build()?;

    lib.define_class(&database)?
        .method("add_coil", &add_coil_fn)?
        .method("add_discrete_input", &add_discrete_input_fn)?
        .method("add_holding_register", &add_holding_register_fn)?
        .method("add_input_register", &add_input_register_fn)?
        .doc("Class used to add, remove, and update values")?
        .build()
}

pub(crate) fn build_handler_map(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> Result<ClassHandle, BindingError> {
    let request_handler = build_request_handler_interface(lib, common)?;

    let database = build_database_class(lib)?;

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

    let map_add_endpoint = lib
        .declare_native_function("map_add_endpoint")?
        .param(
            "map",
            Type::ClassRef(device_map.clone()),
            "map to which the endpoint will be added",
        )?
        .param("unit_id", Type::Uint8, "Unit id of the endpoint")?
        .param(
            "handler",
            Type::Interface(request_handler),
            "callback interface for handling read and write operations for this device",
        )?
        .return_type(ReturnType::Type(
            Type::ClassRef(database.declaration.clone()),
            "Pointer to the database instance for this endpoint, or NULL if it could not be created b/c of a duplicate unit id".into(),
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

pub(crate) fn build_request_handler_interface(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> Result<InterfaceHandle, BindingError> {
    let write_result = lib.declare_native_struct("WriteResult")?;
    let write_result = lib
        .define_native_struct(&write_result)?
        .add("success", Type::Bool, "true if the operation was successful, false otherwise. Error details found in the exception field.")?
        .add("exception", Type::Enum(common.exception.clone()), "exception enumeration. If undefined, look at the raw value")?
        .add("raw_exception", Type::Uint8, "Raw exception value when 'exception' field is Undefined")?
        .doc("Result struct describing if an operation was successful or not. Exception codes are returned to the client")?
        .build()?;

    lib.define_interface(
        "RequestHandler",
        "Interface used to handle read and write requests received from the client",
    )?
    // --- write single coil ---
    .callback(
        "write_single_coil",
        "write a single coil received from the client",
    )?
    .param("value", Type::Bool, "Value of the coil to write")?
    .param("index", Type::Uint16, "Index of the coil")?
    .return_type(ReturnType::Type(
        Type::Struct(write_result.clone()),
        "struct describing the result of the operation".into(),
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
        Type::Struct(write_result.clone()),
        "struct describing the result of the operation".into(),
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
        Type::Struct(write_result.clone()),
        "struct describing the result of the operation".into(),
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
        Type::Struct(write_result),
        "struct describing the result of the operation".into(),
    ))?
    .build()?
    // -------------------------------
    .destroy_callback("destroy")?
    .build()
}
