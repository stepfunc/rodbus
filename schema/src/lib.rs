use crate::common::CommonDefinitions;
use oo_bindgen::{BindingError, Library, LibraryBuilder, Version};

mod channel;
mod common;
mod enums;
mod logging;
mod runtime;

pub fn build() -> Result<Library, BindingError> {
    let mut lib = LibraryBuilder::new("rodbus", Version::new(0, 1, 0));

    // not coupled to any other parts of the API
    logging::define_logging(&mut lib)?;

    let common = CommonDefinitions::build(&mut lib)?;

    channel::build_channel_class(&mut lib, &common)?;

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
