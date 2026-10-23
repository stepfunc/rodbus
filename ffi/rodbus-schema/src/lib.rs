// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

//! FFI oo-bindgen schema for Rodbus

use std::path::PathBuf;

use crate::common::CommonDefinitions;
use oo_bindgen::model::*;

mod client;
mod common;
mod decoding;
mod server;

// derived from Cargo.toml
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Build the library schema
pub fn build_lib() -> BackTraced<Library> {
    let info = LibraryInfo {
        description: "Safe and fast Modbus library".to_string(),
        project_url: "https://stepfunc.io/products/libraries/modbus/".to_string(),
        repository: "stepfunc/rodbus".to_string(),
        license_name: "Custom license".to_string(),
        license_description: [
            "Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.",
            "No use to design, develop, specify, test, validate, or improve Other Implementations,",
            "subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.",
            "Text and data mining rights, including for AI training, are reserved under",
            "Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.",
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

    sfio_tracing_ffi::define(&mut builder, common.error_type.clone())?;

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
