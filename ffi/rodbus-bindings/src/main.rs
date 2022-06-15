use std::path::PathBuf;
use std::rc::Rc;

pub fn main() {
    let library = rodbus_schema::build_lib().unwrap();

    let builder_settings = ci_script::BindingBuilderSettings {
        ffi_target_name: "rodbus-ffi",
        ffi_name: "rodbus_ffi",
        ffi_path: PathBuf::from("ffi/rodbus-ffi"),
        java_group_id: "io.stepfunc",
        destination_path: PathBuf::from("ffi/bindings"),
        library: Rc::new(library),
    };

    ci_script::run(builder_settings);
}
