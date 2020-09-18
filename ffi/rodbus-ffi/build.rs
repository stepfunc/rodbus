fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let lib = rodbus_schema::build().unwrap();
    rust_oo_bindgen::RustCodegen::new(&lib).generate().unwrap();
}
