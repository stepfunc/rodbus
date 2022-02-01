use crate::common::CommonDefinitions;

use oo_bindgen::model::*;

pub(crate) fn build(lib: &mut LibraryBuilder, common: &CommonDefinitions) -> BackTraced<()> {
    let _server = build_server(lib, common)?;
    Ok(())
}

pub(crate) fn build_server(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> BackTraced<ClassHandle> {
    let database = build_database_class(lib, common)?;

    let db_update_callback = lib
        .define_interface(
            "database_callback",
            "Callback used to access the internal database while it is locked",
        )?
        .begin_callback("callback", "callback function")?
        .param(
            "database",
            database.declaration.clone(),
            "Database on which to perform updates",
        )?
        .enable_functional_transform()
        .end_callback()?
        .build_sync()?;

    let handler_map = build_handler_map(
        lib,
        &database.declaration(),
        db_update_callback.clone(),
        common,
    )?;
    let tls_server_config = build_tls_server_config(lib, common)?;
    let authorization_handler = build_authorization_handler(lib, common)?;

    let server = lib.declare_class("server")?;

    let tcp_constructor = lib
        .define_function("server_create_tcp")?
        .param(
            "runtime",
            common.runtime_handle.clone(),
            "runtime on which to spawn the server",
        )?
        .param("address", StringType, "IPv4 or IPv6 host/port string")?
        .param("max_sessions", Primitive::U16, "Maximum number of concurrent sessions")?
        .param(
            "endpoints",
            handler_map.declaration(),
            "Map of endpoints which is emptied upon passing to this function",
        )?
        .param("decode_level", common.decode_level.clone(), "Decode levels for this server")?
        .returns(server.clone(), "TCP server instance")?
        .fails_with(common.error_type.clone())?
        .doc(doc("Launch a TCP server.")
            .details("Recommended port for Modbus is 502.")
            .details("When the maximum number of concurrent sessions is reached, the oldest session is closed."))?
            .build_static("create_tcp")?;

    let rtu_constructor = lib
        .define_function("server_create_rtu")?
        .param(
            "runtime",
            common.runtime_handle.clone(),
            "runtime on which to spawn the server",
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
            "endpoints",
            handler_map.declaration.clone(),
            "map of endpoints which is emptied upon passing to this function",
        )?
        .param(
            "decode_level",
            common.decode_level.clone(),
            "Decode levels for this server",
        )?
        .returns(server.clone(), "RTU server instance")?
        .fails_with(common.error_type.clone())?
        .doc("Launch a RTU server.")?
        .build_static("create_rtu")?;

    let tls_constructor = lib
        .define_function("server_create_tls")?
        .param(
            "runtime",
            common.runtime_handle.clone(),
            "runtime on which to spawn the server",
        )?
        .param("address", StringType, "IPv4 or IPv6 host/port string")?
        .param("max_sessions", Primitive::U16, "Maximum number of concurrent sessions")?
        .param(
            "endpoints",
            handler_map.declaration.clone(),
            "map of endpoints which is emptied upon passing to this function",
        )?
        .param(
            "tls_config",
            tls_server_config,
            "TLS server configuration",
        )?
        .param(
            "authorization_handler",
        authorization_handler,
            "Authorization handler"
        )?
        .param("decode_level", common.decode_level.clone(), "Decode levels for this server")?
        .returns(server.clone(), "Modbus Security (TLS) server instance")?
        .fails_with(common.error_type.clone())?
        .doc(doc("Launch a Modbus Security (TLS) server.")
            .details("Recommended port for Modbus Security is 802.")
            .details("When the maximum number of concurrent sessions is reached, the oldest session is closed."))?
        .build_static("create_tls")?;

    let destructor = lib.define_destructor(
        server.clone(),
        "Shutdown and release all resources of a running server",
    )?;

    let update_fn = lib
        .define_method("update_database", server.clone())?
        .param("unit_id", Primitive::U8, "Unit id of the database to update")?
        .param("transaction", db_update_callback, "Callback invoked when a lock has been acquired")?
        .fails_with(common.error_type.clone())?
        .doc("Update the database associated with a particular unit id. If the unit id exists, lock the database and call user code to perform the transaction")?
        .build()?;

    let server = lib.define_class(&server)?
        .static_method(tcp_constructor)?
        .static_method(rtu_constructor)?
        .static_method(tls_constructor)?
        .method(update_fn)?
        .destructor(destructor)?
        .custom_destroy("shutdown")?
        .doc("Handle to the running server. The server remains alive until this reference is destroyed")?
        .build()?;

    Ok(server)
}

fn build_add_method(
    lib: &mut LibraryBuilder,
    db: &ClassDeclarationHandle,
    snake_name: &str,
    value_type: Primitive,
) -> BackTraced<MethodHandle> {
    let spaced_name = snake_name.replace('_', " ");

    let method = lib
        .define_method(format!("add_{}", snake_name), db.clone())?
        .param(
            "index",
            Primitive::U16,
            format!("Address of the {}", spaced_name),
        )?
        .param(
            "value",
            value_type,
            format!("Initial value of the {}", spaced_name),
        )?
        .returns(Primitive::Bool, "true if the value is new, false otherwise")?
        .doc(format!("Add a new {} to the database", spaced_name))?
        .build()?;

    Ok(method)
}

fn build_get_method(
    lib: &mut LibraryBuilder,
    db: &ClassDeclarationHandle,
    snake_name: &str,
    value_type: Primitive,
    error_type: &ErrorTypeHandle,
) -> BackTraced<MethodHandle> {
    let spaced_name = snake_name.replace('_', " ");

    let method = lib
        .define_method(format!("get_{}", snake_name), db.clone())?
        .param(
            "index",
            Primitive::U16,
            format!("Address of the {}", spaced_name),
        )?
        .returns(value_type, "Current value of the point")?
        .fails_with(error_type.clone())?
        .doc(format!(
            "Get the current {} value of the database",
            spaced_name
        ))?
        .build()?;

    Ok(method)
}

fn build_delete_method(
    lib: &mut LibraryBuilder,
    db: &ClassDeclarationHandle,
    snake_name: &str,
) -> BackTraced<MethodHandle> {
    let spaced_name = snake_name.replace('_', " ");

    let method = lib
        .define_method(format!("delete_{}", snake_name), db.clone())?
        .param(
            "index",
            Primitive::U16,
            format!("Address of the {}", spaced_name),
        )?
        .returns(Primitive::Bool, "true if the value is new, false otherwise")?
        .doc(format!(
            "Remove a {} address from the database",
            spaced_name
        ))?
        .build()?;

    Ok(method)
}

fn build_update_method(
    lib: &mut LibraryBuilder,
    db: &ClassDeclarationHandle,
    snake_name: &str,
    value_type: Primitive,
) -> BackTraced<MethodHandle> {
    let spaced_name = snake_name.replace('_', " ");

    let method = lib
        .define_method(format!("update_{}", snake_name), db.clone())?
        .param(
            "index",
            Primitive::U16,
            format!("Address of the {}", spaced_name),
        )?
        .param(
            "value",
            value_type,
            format!("New value of the {}", spaced_name),
        )?
        .returns(
            Primitive::Bool,
            "true if the address is defined, false otherwise",
        )?
        .doc(format!(
            "Update the current value of a {} in the database",
            spaced_name
        ))?
        .build()?;

    Ok(method)
}

fn build_database_class(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> BackTraced<ClassHandle> {
    let database = lib.declare_class("database")?;

    let add_coil_method = build_add_method(lib, &database, "coil", Primitive::Bool)?;
    let add_discrete_input_method =
        build_add_method(lib, &database, "discrete_input", Primitive::Bool)?;
    let add_holding_register_method =
        build_add_method(lib, &database, "holding_register", Primitive::U16)?;
    let add_input_register_method =
        build_add_method(lib, &database, "input_register", Primitive::U16)?;

    let get_coil_method =
        build_get_method(lib, &database, "coil", Primitive::Bool, &common.error_type)?;
    let get_discrete_input_method = build_get_method(
        lib,
        &database,
        "discrete_input",
        Primitive::Bool,
        &common.error_type,
    )?;
    let get_holding_register_method = build_get_method(
        lib,
        &database,
        "holding_register",
        Primitive::U16,
        &common.error_type,
    )?;
    let get_input_register_method = build_get_method(
        lib,
        &database,
        "input_register",
        Primitive::U16,
        &common.error_type,
    )?;

    let update_coil_method = build_update_method(lib, &database, "coil", Primitive::Bool)?;
    let update_discrete_input_method =
        build_update_method(lib, &database, "discrete_input", Primitive::Bool)?;
    let update_holding_register_method =
        build_update_method(lib, &database, "holding_register", Primitive::U16)?;
    let update_input_register_method =
        build_update_method(lib, &database, "input_register", Primitive::U16)?;

    let delete_coil_method = build_delete_method(lib, &database, "coil")?;
    let delete_discrete_input_method = build_delete_method(lib, &database, "discrete_input")?;
    let delete_holding_register_method = build_delete_method(lib, &database, "holding_register")?;
    let delete_input_register_method = build_delete_method(lib, &database, "input_register")?;

    let class = lib
        .define_class(&database)?
        // add methods
        .method(add_coil_method)?
        .method(add_discrete_input_method)?
        .method(add_holding_register_method)?
        .method(add_input_register_method)?
        // get methods
        .method(get_coil_method)?
        .method(get_discrete_input_method)?
        .method(get_holding_register_method)?
        .method(get_input_register_method)?
        // update methods
        .method(update_coil_method)?
        .method(update_discrete_input_method)?
        .method(update_holding_register_method)?
        .method(update_input_register_method)?
        // delete methods
        .method(delete_coil_method)?
        .method(delete_discrete_input_method)?
        .method(delete_holding_register_method)?
        .method(delete_input_register_method)?
        // docs
        .doc("Class used to add, remove, and update values")?
        .build()?;

    Ok(class)
}

fn build_handler_map(
    lib: &mut LibraryBuilder,
    database: &ClassDeclarationHandle,
    db_update_callback: SynchronousInterface,
    common: &CommonDefinitions,
) -> BackTraced<ClassHandle> {
    let write_handler = build_write_handler_interface(lib, database, common)?;

    let device_map = lib.declare_class("device_map")?;

    let constructor = lib
        .define_constructor(device_map.clone())?
        .doc("Create a device map that will be used to bind devices to a server endpoint")?
        .build()?;

    let destructor = lib.define_destructor(
        device_map.clone(),
        "Destroy a previously created device map",
    )?;

    let map_add_endpoint = lib
        .define_method("add_endpoint", device_map.clone())?
        .param("unit_id", Primitive::U8, "Unit id of the endpoint")?
        .param(
            "handler",
            write_handler,
            "Callback interface for handling write operations for this device",
        )?
        .param(
            "configure",
            db_update_callback,
            "One-time callback interface configuring the initial state of the database",
        )?
        .returns(
            Primitive::Bool,
            "True if the unit id doesn't already exists, false otherwise",
        )?
        .doc("Add an endpoint to the map")?
        .build()?;

    let class = lib
        .define_class(&device_map)?
        .constructor(constructor)?
        .destructor(destructor)?
        .method(map_add_endpoint)?
        .doc("Maps endpoint handlers to Modbus address")?
        .build()?;

    Ok(class)
}

fn build_authorization_handler(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> BackTraced<AsynchronousInterface> {
    let auth_result = lib
        .define_enum("authorization_result")?
        .push(
            "authorized",
            "Client is authorized to perform the operation",
        )?
        .push(
            "not_authorized",
            "Client is non authorized to perform the operation",
        )?
        .doc("Authorization result used by {interface:authorization_handler}")?
        .build()?;

    let definition = lib
        .define_interface(
            "authorization_handler",
            "Modbus Security authorization handler",
        )?
        .begin_callback("read_coils", "Authorize a Read Discrete Inputs request")?
        .param("unit_id", Primitive::U8, "Target unit ID")?
        .param("range", common.address_range.clone(), "Range to read")?
        .param("role", StringType, "Authenticated Modbus role")?
        .returns(auth_result.clone(), "Authorization result")?
        .end_callback()?
        .begin_callback(
            "read_discrete_inputs",
            "Authorize a Read Discrete Inputs request",
        )?
        .param("unit_id", Primitive::U8, "Target unit ID")?
        .param("range", common.address_range.clone(), "Range to read")?
        .param("role", StringType, "Authenticated Modbus role")?
        .returns(auth_result.clone(), "Authorization result")?
        .end_callback()?
        .begin_callback(
            "read_holding_registers",
            "Authorize a Read Holding Registers request",
        )?
        .param("unit_id", Primitive::U8, "Target unit ID")?
        .param("range", common.address_range.clone(), "Range to read")?
        .param("role", StringType, "Authenticated Modbus role")?
        .returns(auth_result.clone(), "Authorization result")?
        .end_callback()?
        .begin_callback(
            "read_input_registers",
            "Authorize a Read Input Registers request",
        )?
        .param("unit_id", Primitive::U8, "Target unit ID")?
        .param("range", common.address_range.clone(), "Range to read")?
        .param("role", StringType, "Authenticated Modbus role")?
        .returns(auth_result.clone(), "Authorization result")?
        .end_callback()?
        .begin_callback("write_single_coil", "Authorize a Write Single Coil request")?
        .param("unit_id", Primitive::U8, "Target unit ID")?
        .param("index", Primitive::U16, "Target index")?
        .param("role", StringType, "Authenticated Modbus role")?
        .returns(auth_result.clone(), "Authorization result")?
        .end_callback()?
        .begin_callback(
            "write_single_register",
            "Authorize a Write Single Register request",
        )?
        .param("unit_id", Primitive::U8, "Target unit ID")?
        .param("index", Primitive::U16, "Target index")?
        .param("role", StringType, "Authenticated Modbus role")?
        .returns(auth_result.clone(), "Authorization result")?
        .end_callback()?
        .begin_callback(
            "write_multiple_coils",
            "Authorize a Write Multiple Coils request",
        )?
        .param("unit_id", Primitive::U8, "Target unit ID")?
        .param("range", common.address_range.clone(), "Range to read")?
        .param("role", StringType, "Authenticated Modbus role")?
        .returns(auth_result.clone(), "Authorization result")?
        .end_callback()?
        .begin_callback(
            "write_multiple_registers",
            "Authorize a Write Multiple Registers request",
        )?
        .param("unit_id", Primitive::U8, "Target unit ID")?
        .param("range", common.address_range.clone(), "Range to read")?
        .param("role", StringType, "Authenticated Modbus role")?
        .returns(auth_result, "Authorization result")?
        .end_callback()?
        .build_async()?;

    Ok(definition)
}

fn build_tls_server_config(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> BackTraced<FunctionArgStructHandle> {
    let min_tls_version_field = Name::create("min_tls_version")?;
    let certificate_mode_field = Name::create("certificate_mode")?;

    let tls_server_config = lib.declare_function_argument_struct("tls_server_config")?;
    let tls_server_config = lib
        .define_function_argument_struct(tls_server_config)?
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
            doc("Optional password if the private key file is encrypted")
                .details("Only PKCS#8 encrypted files are supported.")
                .details("Pass empty string if the file is not encrypted."),
        )?
        .add(
            &min_tls_version_field,
            common.min_tls_version.clone(),
            "Minimum TLS version allowed",
        )?
        .add(
            &certificate_mode_field,
            common.certificate_mode.clone(),
            "Certficate validation mode",
        )?
        .doc("TLS server configuration")?
        .end_fields()?
        .begin_initializer(
            "init",
            InitializerType::Normal,
            "Initialize a TLS client configuration",
        )?
        .default_variant(&min_tls_version_field, "v12")?
        .default_variant(&certificate_mode_field, "authority_based")?
        .end_initializer()?
        .build()?;

    Ok(tls_server_config)
}

fn build_write_handler_interface(
    lib: &mut LibraryBuilder,
    database: &ClassDeclarationHandle,
    common: &CommonDefinitions,
) -> BackTraced<AsynchronousInterface> {
    let write_result = build_write_result_struct(lib, common)?;

    let interface = lib
        .define_interface(
            "write_handler",
            "Interface used to handle write requests received from the client",
        )?
        // --- write single coil ---
        .begin_callback(
            "write_single_coil",
            "Write a single coil received from the client",
        )?
        .param("index", Primitive::U16, "Index of the coil")?
        .param("value", Primitive::Bool, "Value of the coil to write")?
        .param(
            "database",
            database.clone(),
            "Database interface for updates",
        )?
        .returns(
            write_result.clone(),
            "Struct describing the result of the operation",
        )?
        .end_callback()?
        // --- write single register ---
        .begin_callback(
            "write_single_register",
            "write a single coil received from the client",
        )?
        .param("index", Primitive::U16, "Index of the register")?
        .param("value", Primitive::U16, "Value of the register to write")?
        .param(
            "database",
            database.clone(),
            "Database interface for updates",
        )?
        .returns(
            write_result.clone(),
            "Struct describing the result of the operation",
        )?
        .end_callback()?
        // --- write multiple coils ---
        .begin_callback(
            "write_multiple_coils",
            "Write multiple coils received from the client",
        )?
        .param("start", Primitive::U16, "Starting address")?
        .param(
            "it",
            common.bit_iterator.clone(),
            "Iterator over coil values",
        )?
        .param(
            "database",
            database.clone(),
            "Database interface for updates",
        )?
        .returns(
            write_result.clone(),
            "Struct describing the result of the operation",
        )?
        .end_callback()?
        // --- write multiple registers ---
        .begin_callback(
            "write_multiple_registers",
            "Write multiple registers received from the client",
        )?
        .param("start", Primitive::U16, "Starting address")?
        .param(
            "it",
            common.register_iterator.clone(),
            "Iterator over register values",
        )?
        .param(
            "database",
            database.clone(),
            "Database interface for updates",
        )?
        .returns(
            write_result,
            "Struct describing the result of the operation",
        )?
        .end_callback()?
        // -------------------------------
        .build_async()?;

    Ok(interface)
}

fn build_write_result_struct(
    lib: &mut LibraryBuilder,
    common: &CommonDefinitions,
) -> BackTraced<UniversalStructHandle> {
    let success_field = Name::create("success")?;
    let exception_field = Name::create("exception")?;
    let raw_exception_field = Name::create("raw_exception")?;

    let write_result = lib.declare_universal_struct("write_result")?;
    let write_result = lib
        .define_universal_struct(write_result)?
        .add(success_field.clone(), Primitive::Bool, "true if the operation was successful, false otherwise. Error details found in the exception field.")?
        .add(exception_field.clone(), common.exception.clone(), "Exception enumeration. If {enum:modbus_exception.unknown}, look at the raw value")?
        .add(raw_exception_field.clone(), Primitive::U8, "Raw exception value when {struct:write_result.exception} field is {enum:modbus_exception.unknown}")?
        .doc("Result struct describing if an operation was successful or not. Exception codes are returned to the client")?
        .end_fields()?
        // success initializer
        .begin_initializer("success_init", InitializerType::Static, "Initialize a {struct:write_result} to indicate a successful write operation")?
        .default(&success_field, true)?
        .default_variant(&exception_field, "unknown")?
        .default(&raw_exception_field, NumberValue::U8(0))?
        .end_initializer()?
        // exception initializer
        .begin_initializer("exception_init", InitializerType::Static, "Initialize a {struct:write_result} to indicate a standard Modbus exception")?
        .default(&success_field, false)?
        .default(&raw_exception_field, NumberValue::U8(0))?
        .end_initializer()?
        // raw exception initializer
        .begin_initializer("raw_exception_init", InitializerType::Static, "Initialize a {struct:write_result} to indicate a non-standard Modbus exception")?
        .default(&success_field, false)?
        .default_variant(&exception_field, "unknown")?
        .end_initializer()?
        .build()?;

    Ok(write_result)
}
