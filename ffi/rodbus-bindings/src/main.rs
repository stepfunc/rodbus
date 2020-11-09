use std::path::Path;

pub fn main() {
    let builder_settings = ci_script::BindingBuilderSettings {
        ffi_name: "rodbus_ffi",
        ffi_path: Path::new("ffi/rodbus-ffi"),
        java_group_id: "io.stepfunc",
        destination_path: Path::new("ffi/bindings"),
        library: &rodbus_schema::build().unwrap(),
    };

    ci_script::run(builder_settings);
}
