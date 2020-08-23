use oo_bindgen::{BindingError, Library, LibraryBuilder};

mod channel;
mod enums;
mod logging;
mod runtime;

pub fn build() -> Result<Library, BindingError> {
    let mut lib = LibraryBuilder::new("rodbus", semver::Version::new(0, 1, 0));
    lib.description("Modbus library in safe Rust")?;

    logging::define_logging(&mut lib)?;

    let _exception = enums::define_exception(&mut lib)?;
    let runtime_class = runtime::build_runtime_class(&mut lib)?;

    let _channel_class = channel::build_channel_class(&mut lib, runtime_class.clone())?;

    Ok(lib.build())
}

#[cfg(test)]
mod tests {
    use crate::build;

    #[test]
    fn builds_library_without_error() {
        build().unwrap();
    }
}
