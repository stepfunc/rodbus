fn main() {
    println!("cargo:rerun-if-changed=build.rs");

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
