use oo_bindgen::model::*;

fn define_log_level_enum(lib: &mut LibraryBuilder) -> BackTraced<EnumHandle> {
    let definition = lib
        .define_enum("log_level")?
        .push("error", "Error log level")?
        .push("warn", "Warning log level")?
        .push("info", "Information log level")?
        .push("debug", "Debugging log level")?
        .push("trace", "Trace log level")?
        .doc(
            doc("Log level")
                .details("Used in {interface:logger.on_message()} callback to identify the log level of a message.")
        )?
        .build()?;

    Ok(definition)
}

fn define_time_format_enum(lib: &mut LibraryBuilder) -> BackTraced<EnumHandle> {
    let definition = lib
        .define_enum("time_format")?
        .push("none", "Don't format the timestamp as part of the message")?
        .push("rfc_3339", "Format the time using RFC 3339")?
        .push(
            "system",
            "Format the time in a human readable format e.g. 'Jun 25 14:27:12.955'",
        )?
        .doc("Describes if and how the time will be formatted in log messages")?
        .build()?;

    Ok(definition)
}

fn define_log_output_format_enum(lib: &mut LibraryBuilder) -> BackTraced<EnumHandle> {
    let definition = lib
        .define_enum("log_output_format")?
        .push("text", "A simple text-based format")?
        .push("json", "Output formatted as JSON")?
        .doc("Describes how each log event is formatted")?
        .build()?;

    Ok(definition)
}

fn define_logging_config_struct(
    lib: &mut LibraryBuilder,
    log_level_enum: EnumHandle,
) -> BackTraced<FunctionArgStructHandle> {
    let logging_config_struct = lib.declare_function_argument_struct("logging_config")?;

    let log_output_format_enum = define_log_output_format_enum(lib)?;
    let time_format_enum = define_time_format_enum(lib)?;

    let level = Name::create("level")?;
    let output_format = Name::create("output_format")?;
    let time_format = Name::create("time_format")?;
    let print_level = Name::create("print_level")?;
    let print_module_info = Name::create("print_module_info")?;

    let logging_config_struct = lib
        .define_function_argument_struct(logging_config_struct)?
        .add(&level, log_level_enum, "logging level")?
        .add(
            &output_format,
            log_output_format_enum,
            "output formatting options",
        )?
        .add(&time_format, time_format_enum, "optional time format")?
        .add(
            &print_level,
            Primitive::Bool,
            "optionally print the log level as part to the message string",
        )?
        .add(
            &print_module_info,
            Primitive::Bool,
            "optionally print the underlying Rust module information to the message string",
        )?
        .doc("Logging configuration options")?
        .end_fields()?
        .begin_initializer(
            "init",
            InitializerType::Normal,
            "Initialize the configuration to default values",
        )?
        .default(&level, "info".default_variant())?
        .default(&output_format, "text".default_variant())?
        .default(&time_format, "system".default_variant())?
        .default(&print_level, true)?
        .default(&print_module_info, false)?
        .end_initializer()?
        .build()?;

    Ok(logging_config_struct)
}

const NOTHING: &str = "nothing";

pub fn define(
    lib: &mut LibraryBuilder,
    error_type: ErrorTypeHandle,
) -> BackTraced<UniversalStructHandle> {
    let log_level_enum = define_log_level_enum(lib)?;

    let logging_config_struct = define_logging_config_struct(lib, log_level_enum.clone())?;

    let log_callback_interface = lib
        .define_interface(
            "logger",
            "Logging interface that receives the log messages and writes them somewhere.",
        )?
        .begin_callback(
            "on_message",
            "Called when a log message was received and should be logged",
        )?
        .param("level", log_level_enum, "Level of the message")?
        .param("message", StringType, "Actual formatted message")?
        .end_callback()?
        .build_async()?;

    let configure_logging_fn = lib
        .define_function("configure_logging")?
        .param(
            "config",
           logging_config_struct,
            "Configuration options for logging"
        )?
        .param(
            "logger",
           log_callback_interface,
            "Logger that will receive each logged message",
        )?
        .fails_with(error_type)?
        .doc(
            doc("Set the callback that will receive all the log messages")
            .details("There is only a single globally allocated logger. Calling this method a second time will return an error.")
            .details("If this method is never called, no logging will be performed.")
        )?
        .build_static("configure")?;

    let _logging_class = lib
        .define_static_class("logging")?
        .static_method(configure_logging_fn)?
        .doc("Provides a static method for configuring logging")?
        .build()?;

    let pdu_decode_level_enum = lib
        .define_enum("pdu_decode_level")?
        .push(NOTHING, "Decode nothing")?
        .push("function_code", "Decode the function code only")?
        .push("data_headers", "Decode the function code and the general description of the data")?
        .push(
            "data_values",
            "Decode the function code, the general description of the data and the actual data values",
        )?
        .doc("Controls how transmitted and received Protocol Data Units (PDUs) are decoded at the INFO log level")?
        .build()?;

    let adu_decode_level_enum = lib
        .define_enum("adu_decode_level")?
        .push(NOTHING, "Log nothing")?
        .push("header", " Decode the header")?
        .push("payload", "Decode the header and the raw payload as hexadecimal")?
        .doc(doc("Controls how transmitted and received transport segments are decoded at the INFO log level").details("On TCP, this is the MBAP decoding. On serial, this controls the serial line PDU."))?
        .build()?;

    let phys_decode_level_enum = lib
        .define_enum("phys_decode_level")?
        .push(NOTHING, "Log nothing")?
        .push(
            "length",
            "Log only the length of data that is sent and received",
        )?
        .push(
            "data",
            "Log the length and the actual data that is sent and received",
        )?
        .doc("Controls how data transmitted at the physical layer (TCP, serial, etc) is logged")?
        .build()?;

    let pdu_field = Name::create("pdu")?;
    let adu_field = Name::create("adu")?;
    let physical_field = Name::create("physical")?;

    let decode_level_struct = lib.declare_universal_struct("decode_level")?;
    let decode_level_struct = lib.define_universal_struct(decode_level_struct)?
        .add(&pdu_field, pdu_decode_level_enum, "Controls the protocol data unit decoding")?
        .add(&adu_field, adu_decode_level_enum, "Controls the application data unit decoding")?
        .add(&physical_field, phys_decode_level_enum, "Controls the logging of physical layer read/write")?
        .doc("Controls the decoding of transmitted and received data at the ADU, PDU, and physical layers")?
        .end_fields()?
        .begin_initializer("init", InitializerType::Normal, "Initialize log levels to defaults")?
        .default_variant(&adu_field, NOTHING)?
        .default_variant(&pdu_field, NOTHING)?
        .default_variant(&physical_field, NOTHING)?
        .end_initializer()?
        .build()?;

    Ok(decode_level_struct)
}
