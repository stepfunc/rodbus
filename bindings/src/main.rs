use std::path::PathBuf;

use c_oo_bindgen::CBindgenConfig;
use dotnet_oo_bindgen::DotnetBindgenConfig;
use java_oo_bindgen::JavaBindgenConfig;
use oo_bindgen::platforms::{Platform, PlatformLocations};
use oo_bindgen::Library;

fn generate_c_bindings(lib: &Library) {
    let mut platforms = PlatformLocations::new();
    platforms.add(
        Platform::current(),
        PathBuf::from("C:\\Users\\Adam\\Documents\\code\\rodbus\\target\\debug\\deps"),
    );

    let config = CBindgenConfig {
        output_dir: PathBuf::from("C:\\Users\\Adam\\Documents\\code\\rodbus\\generated\\c"),
        ffi_name: "rodbus_ffi_new".to_string(),
        platforms,
        generate_doc: false,
    };

    c_oo_bindgen::generate_c_package(&lib, &config).unwrap();
}

fn generate_csharp_bindings(lib: &Library) {
    let mut platforms = PlatformLocations::new();
    platforms.add(
        Platform::current(),
        PathBuf::from("C:\\Users\\Adam\\Documents\\code\\rodbus\\target\\debug\\deps"),
    );

    let config = DotnetBindgenConfig {
        output_dir: PathBuf::from("C:\\Users\\Adam\\Documents\\code\\rodbus\\generated\\dotnet"),
        ffi_name: "rodbus_ffi_new".to_string(),
        platforms,
    };

    dotnet_oo_bindgen::generate_dotnet_bindings(&lib, &config).unwrap();
}

fn generate_java_bindings(lib: &Library) {
    let mut platforms = PlatformLocations::new();
    platforms.add(
        Platform::current(),
        PathBuf::from("C:\\Users\\Adam\\Documents\\code\\rodbus\\target\\debug\\deps"),
    );

    let config = JavaBindgenConfig {
        output_dir: PathBuf::from("C:\\Users\\Adam\\Documents\\code\\rodbus\\generated\\java"),
        ffi_name: "rodbus_ffi_new".to_string(),
        group_id: "io.stepfunc".to_string(),
        platforms,
    };

    java_oo_bindgen::generate_java_bindings(&lib, &config).unwrap();
}

pub fn main() {
    let lib = rodbus_ffi_schema::build().unwrap();

    generate_c_bindings(&lib);
    generate_csharp_bindings(&lib);
    generate_java_bindings(&lib);
}
