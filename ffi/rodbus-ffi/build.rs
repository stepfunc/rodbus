use std::env;
use std::io::Write;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let mut file =
        std::fs::File::create(Path::new(&env::var_os("OUT_DIR").unwrap()).join("tracing.rs"))
            .unwrap();
    file.write_all(tracing_ffi_schema::get_impl_file().as_bytes())
        .unwrap();

    match rodbus_schema::build_lib() {
        Ok(lib) => {
            rust_oo_bindgen::RustCodegen::new(&lib).generate().unwrap();
        }
        Err(err) => {
            eprintln!("rodbus model error: {}", err);
            std::process::exit(-1);
        }
    };
}
