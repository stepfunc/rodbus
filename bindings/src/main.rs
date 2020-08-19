use std::path::PathBuf;

use c_oo_bindgen::CBindgenConfig;
use oo_bindgen::platforms::{PlatformLocations, Platform};
use oo_bindgen::Library;

fn generate_c_bindings(lib: &Library) {

    let mut platforms = PlatformLocations::new();
    platforms.add(Platform::current(), PathBuf::from("C:\\Users\\Adam\\Documents\\code\\rodbus\\target\\debug\\deps"));

    let config = CBindgenConfig {
        output_dir: PathBuf::from("C:\\Users\\Adam\\Documents\\code\\rodbus\\generated\\c"),
        ffi_name: "rodbus_ffi_new".to_string(),
        platforms,
        generate_doc: false,
    };

    c_oo_bindgen::generate_c_package(&lib, &config).unwrap();
}


pub fn main() {
    let lib = rodbus_ffi_schema::build().unwrap();

    generate_c_bindings(&lib);
}