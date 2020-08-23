use oo_bindgen::class::ClassHandle;
use oo_bindgen::native_function::{ReturnType, Type};
use oo_bindgen::{BindingError, LibraryBuilder};

pub fn build_runtime_class(lib: &mut LibraryBuilder) -> Result<ClassHandle, BindingError> {
    let runtime_class = lib.declare_class("Runtime")?;

    let config_struct = lib.declare_native_struct("RuntimeConfig")?;
    let config_struct = lib
        .define_native_struct(&config_struct)?
        .add(
            "num_core_threads",
            Type::Uint16,
            "Number of runtime threads to spawn. For a guess of the number of CPUs, use 0.",
        )?
        .doc("Runtime configuration")?
        .build()?;

    // Declare the native functions
    let new_fn = lib
        .declare_native_function("runtime_new")?
        .param(
            "config",
            Type::StructRef(config_struct.declaration()),
            "Runtime configuration",
        )?
        .return_type(ReturnType::new(
            Type::ClassRef(runtime_class.clone()),
            "Handle to the created runtime, NULL if an error occurred",
        ))?
        .doc("Create a new runtime")?
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
        .doc("Event-queue based runtime handle")?
        .build()?;

    Ok(runtime_class)
}
