use std::path::PathBuf;

use crate::common::CommonDefinitions;
use oo_bindgen::model::*;

mod client;
mod common;
mod logging;
mod runtime;
mod server;

// derived from Cargo.toml
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn build_lib() -> BackTraced<Library> {
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
        logo_png: include_bytes!("../../../sfio_logo.png"),
    };

    let settings = LibrarySettings::create(
        "rodbus",
        "rodbus",
        ClassSettings::default(),
        IteratorSettings::default(),
        CollectionSettings::default(),
        FutureSettings::default(),
        InterfaceSettings::default(),
    )?;

    let mut builder = LibraryBuilder::new(Version::parse(VERSION).unwrap(), info, settings);

    let common = CommonDefinitions::build(&mut builder)?;

    client::build(&mut builder, &common)?;
    server::build(&mut builder, &common)?;

    let library = builder.build()?;

    Ok(library)
}

#[cfg(test)]
mod tests {
    #[test]
    fn builds_library_without_error() {
        crate::build_lib().unwrap();
    }
}
