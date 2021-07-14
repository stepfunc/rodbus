use oo_bindgen::class::ClassHandle;
use oo_bindgen::error_type::ErrorType;
use oo_bindgen::native_function::{ReturnType, Type};
use oo_bindgen::native_struct::StructElementType;
use oo_bindgen::{doc, BindingError, LibraryBuilder};

pub fn build_runtime_class(
    lib: &mut LibraryBuilder,
    error_type: ErrorType,
) -> Result<ClassHandle, BindingError> {
    let runtime_class = lib.declare_class("Runtime")?;

    let config_struct = lib.declare_native_struct("RuntimeConfig")?;
    let config_struct = lib
        .define_native_struct(&config_struct)?
        .add(
            "num_core_threads",
            StructElementType::Uint16(Some(0)),
            doc("Number of runtime threads to spawn. For a guess of the number of CPUs, use 0.")
            .details("Even if tons of connections are expected, it is preferred to use a value around the number of CPU cores for better performances. The library uses an efficient thread pool polling mechanism."),
        )?
        .doc("Runtime configuration")?
        .build()?;

    // Declare the native functions
    let new_fn = lib
        .declare_native_function("runtime_new")?
        .param(
            "config",
            Type::Struct(config_struct),
            "Runtime configuration",
        )?
        .return_type(ReturnType::new(
            Type::ClassRef(runtime_class.clone()),
            "Handle to the created runtime, NULL if an error occurred",
        ))?
        .fails_with(error_type)?
        .doc(
            doc("Creates a new runtime for running the protocol stack.")
            .warning("The runtime should be kept alive for as long as it's needed and it should be released with {class:Runtime.[destructor]}")
        )?
        .build()?;

    let destroy_fn = lib
        .declare_native_function("runtime_destroy")?
        .param("runtime", Type::ClassRef(runtime_class.clone()), "Runtime to destroy")?
        .return_type(ReturnType::void())?
        .doc("Destroy a runtime. This method will gracefully wait for all asynchronous operation to end before returning")?
        .build()?;

    // Declare the object-oriented class
    let runtime_class = lib
        .define_class(&runtime_class)?
        .constructor(&new_fn)?
        .destructor(&destroy_fn)?
        .custom_destroy("Shutdown")?
        .doc("Event-queue based runtime handle")?
        .build()?;

    Ok(runtime_class)
}
