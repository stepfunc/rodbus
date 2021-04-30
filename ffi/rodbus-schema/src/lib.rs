use std::path::PathBuf;

use crate::common::CommonDefinitions;
use oo_bindgen::{BindingError, DeveloperInfo, Library, LibraryBuilder, LibraryInfo, Version};

mod client;
mod common;
mod enums;
mod logging;
mod runtime;
mod server;

pub fn build() -> Result<Library, BindingError> {
    let info = LibraryInfo {
        description: "Safe and fast Modbus library".to_string(),
        project_url: "https://stepfunc.io/products/libraries/modbus/".to_string(),
        repository: "stepfunc/modbus".to_string(),
        license_name: "Custom license".to_string(),
        license_description: [
            "This library is provided under the terms of a non-commercial license.",
            "",
            "Please refer to the source repository for details:",
            "",
            "https://github.com/stepfunc/rodbus/blob/master/LICENSE.txt",
            "",
            "Please contact Step Function I/O if you are interested in commercial license:",
            "",
            "info@stepfunc.io",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        license_path: PathBuf::from("LICENSE.txt"),
        developers: vec![
            DeveloperInfo {
                name: "J. Adam Crain".to_string(),
                email: "adam@stepfunc.io".to_string(),
                organization: "Step Function I/O".to_string(),
                organization_url: "https://stepfunc.io/".to_string(),
            },
            DeveloperInfo {
                name: "Émile Grégoire".to_string(),
                email: "emile@stepfunc.io".to_string(),
                organization: "Step Function I/O".to_string(),
                organization_url: "https://stepfunc.io/".to_string(),
            },
        ],
    };
    let mut lib = LibraryBuilder::new("rodbus", Version::parse(rodbus::VERSION).unwrap(), info);

    // not coupled to any other parts of the API
    logging::define_logging(&mut lib)?;

    let common = CommonDefinitions::build(&mut lib)?;

    client::build(&mut lib, &common)?;
    server::build(&mut lib, &common)?;

    lib.build()
}

#[cfg(test)]
mod tests {
    use crate::build;

    #[test]
    fn builds_library_without_error() {
        build().unwrap();
    }
}
