use oo_bindgen::{LibraryBuilder, Library, BindingError};

mod channel;
mod runtime;

pub fn build() -> Result<Library, BindingError> {
    let mut lib = LibraryBuilder::new("rodbus", semver::Version::new(0, 1, 0));
    lib.description("Modbus library in safe Rust")?;

    let runtime_class = runtime::build_runtime_class(&mut lib)?;

    let _channel_class = channel::build_channel_class(&mut lib, runtime_class.clone())?;

    Ok(lib.build())
}
