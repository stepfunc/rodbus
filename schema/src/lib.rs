use oo_bindgen::{LibraryBuilder, Library, BindingError};

mod runtime;

pub fn build() -> Result<Library, BindingError> {
    let mut lib = LibraryBuilder::new("rodbus", semver::Version::new(0, 1, 0));
    lib.description("Modbus library in safe Rust")?;

    let _runtime_class = runtime::build_runtime_class(&mut lib)?;

    Ok(lib.build())
}

