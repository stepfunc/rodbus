use oo_bindgen::class::ClassHandle;
use oo_bindgen::native_function::Type;
use oo_bindgen::native_struct::NativeStructHandle;
use oo_bindgen::{BindingError, LibraryBuilder};

pub(crate) struct CommonDefinitions {
    pub(crate) runtime_handle: ClassHandle,
    pub(crate) error_info: NativeStructHandle,
    pub(crate) address_range: NativeStructHandle,
    pub(crate) request_param: NativeStructHandle,
    pub(crate) bit: NativeStructHandle,
    pub(crate) register: NativeStructHandle,
}

impl CommonDefinitions {
    pub(crate) fn build(lib: &mut LibraryBuilder) -> Result<CommonDefinitions, BindingError> {
        Ok(Self {
            runtime_handle: crate::runtime::build_runtime_class(lib)?,
            error_info: build_error_info(lib)?,
            address_range: build_address_range(lib)?,
            request_param: build_request_param(lib)?,
            bit: build_bit(lib)?,
            register: build_register(lib)?,
        })
    }
}

fn build_bit(lib: &mut LibraryBuilder) -> Result<NativeStructHandle, BindingError> {
    let bit = lib.declare_native_struct("Bit")?;
    lib.define_native_struct(&bit)?
        .add("index", Type::Uint16, "index of bit")?
        .add("value", Type::Bool, "value of the bit")?
        .doc("index/value tuple of a bit type")?
        .build()
}

fn build_register(lib: &mut LibraryBuilder) -> Result<NativeStructHandle, BindingError> {
    let bit = lib.declare_native_struct("Register")?;
    lib.define_native_struct(&bit)?
        .add("index", Type::Uint16, "index of register")?
        .add("value", Type::Uint16, "value of the register")?
        .doc("index/value tuple of a register type")?
        .build()
}

fn build_address_range(lib: &mut LibraryBuilder) -> Result<NativeStructHandle, BindingError> {
    let info = lib.declare_native_struct("AddressRange")?;
    let info = lib
        .define_native_struct(&info)?
        .add("start", Type::Uint16, "Starting address of the range")?
        .add("count", Type::Uint16, "Number of addresses in the range")?
        .doc("Range of 16-bit addresses")?
        .build()?;

    Ok(info)
}

fn build_request_param(lib: &mut LibraryBuilder) -> Result<NativeStructHandle, BindingError> {
    let param = lib.declare_native_struct("RequestParam")?;
    let param = lib
        .define_native_struct(&param)?
        .add("unit_id", Type::Uint8, "Modbus address for the request")?
        .add(
            "timeout_ms",
            Type::Uint32,
            "Response timeout for the request in milliseconds",
        )?
        .doc("Address and timeout parameters for requests")?
        .build()?;

    Ok(param)
}

fn build_error_info(lib: &mut LibraryBuilder) -> Result<NativeStructHandle, BindingError> {
    let status = crate::enums::define_status(lib)?;
    let exception = crate::enums::define_exception(lib)?;

    let info = lib.declare_native_struct("ErrorInfo")?;
    let info = lib
        .define_native_struct(&info)?
        .add(
            "summary",
            Type::Enum(status),
            "top level status code for the operation",
        )?
        .add(
            "exception",
            Type::Enum(exception),
            "exception code returned by the server when status == Exception",
        )?
        .add(
            "raw_exception",
            Type::Uint8,
            "raw exception code returned by the server",
        )?
        .doc("Summarizes the success or failure of an operation")?
        .build()?;

    Ok(info)
}
