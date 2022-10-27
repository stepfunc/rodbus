use std::path::PathBuf;
use std::rc::Rc;

pub fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    let library = rodbus_schema::build_lib().unwrap();

    let builder_settings = oo_bindgen::cli::BindingBuilderSettings {
        ffi_target_name: "rodbus-ffi",
        jni_target_name: "rodbus-ffi-java",
        ffi_name: "rodbus_ffi",
        ffi_path: PathBuf::from("ffi/rodbus-ffi"),
        java_group_id: "io.stepfunc",
        destination_path: PathBuf::from("ffi/bindings"),
        library: Rc::new(library),
    };

    oo_bindgen::cli::run(builder_settings);
}
