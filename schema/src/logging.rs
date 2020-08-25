use oo_bindgen::native_enum::NativeEnum;
use oo_bindgen::native_function::{ReturnType, Type};
use oo_bindgen::{BindingError, Handle, LibraryBuilder};

fn define_log_level(lib: &mut LibraryBuilder) -> Result<Handle<NativeEnum>, BindingError> {
    lib.define_native_enum("LogLevel")?
        .variant("Error", 1, "serious errors")?
        .variant("Warn", 2, "atypical situations")?
        .variant("Info", 3, "useful information")?
        .variant("Debug", 4, "lower priority information")?
        .variant(
            "Trace",
            5,
            "very low priority, often extremely verbose, information",
        )?
        .doc("enum representing the available verbosity levels of the logger")?
        .build()
}

pub(crate) fn define_logging(lib: &mut LibraryBuilder) -> Result<(), BindingError> {
    let level = define_log_level(lib)?;

    let log_handler_interface = lib
        .define_interface("LogHandler", "Logging interface")?
        .callback("on_message", "Called when a message should be logged")?
        .param("level", Type::Enum(level.clone()), "Level of the message")?
        .param("message", Type::String, "Formatted log message")?
        .arg("arg")?
        .return_type(ReturnType::void())?
        .build()?
        .destroy_callback("on_destroy")?
        .arg("arg")?
        .build()?;

    let set_logger_fn = lib
        .declare_native_function("set_log_handler")?
        .param(
            "callback",
            Type::Interface(log_handler_interface),
            "Handler that will receive all log messages",
        )?
        .return_type(ReturnType::Type(
            Type::Bool,
            "true if successful, false otherwise".into(),
        ))?
        .doc("set the callback that will handle log messages")?
        .build()?;

    let set_max_level_fn = lib
        .declare_native_function("set_max_log_level")?
        .param("level", Type::Enum(level), "maximum level to be logged")?
        .return_type(ReturnType::void())?
        .doc("Set the maximum level that will be logged")?
        .build()?;

    let logging_class = lib.declare_class("Logging")?;

    let _ = lib
        .define_class(&logging_class)?
        .static_method("SetHandler", &set_logger_fn)?
        .static_method("SetMaxLogLevel", &set_max_level_fn)?
        .doc("Helper functions for logging")?
        .build()?;

    Ok(())
}
