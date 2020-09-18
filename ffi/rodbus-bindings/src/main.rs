use std::path::Path;

pub fn main() {
    let builder_settings = ci_script::BindingBuilderSettings {
        ffi_name: "rodbus_ffi",
        destination_path: Path::new("ffi/bindings"),
        library: &rodbus_schema::build().unwrap(),
    };

    ci_script::run(builder_settings);
}
