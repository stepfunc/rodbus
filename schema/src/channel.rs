use oo_bindgen::{BindingError, LibraryBuilder, Handle};
use oo_bindgen::native_function::{Type, ReturnType};
use oo_bindgen::class::{ClassHandle, Class};

pub fn build_channel_class(lib: &mut LibraryBuilder, runtime: Handle<Class>) -> Result<ClassHandle, BindingError> {

    let channel = lib.declare_class("Channel")?;

    let create_tcp_client_fn = lib.declare_native_function("create_tcp_client")?;

    let create_tcp_client_fn = create_tcp_client_fn
        .param("runtime", Type::ClassRef(runtime.declaration.clone()), "runtime on which to create the channel")?
        .param("address", Type::String, "IP address of remote host")?
        .param("max_queued_requests", Type::Uint16, "Maximum number of requests to queue before failing the next request")?
        .return_type(ReturnType::Type(Type::ClassRef(channel.clone()), "pointer to the created channel or NULL if an error occurred".into()))?
        .doc("create a new tcp channel instance")?
        .build()?;

    let destroy_tcp_client_fn = lib.declare_native_function("destroy_channel")?;

    let destroy_tcp_client_fn  = destroy_tcp_client_fn
        .param("channel", Type::ClassRef(channel.clone()), "channel to destroy")?
        .return_type(ReturnType::Void)?
        .doc("destroy a channel instance")?
        .build()?;

    let channel = lib.define_class(&channel)?
        .static_method("create_tcp_client", &create_tcp_client_fn)?
        .destructor(&destroy_tcp_client_fn)?
        .doc("Abstract representation of a channel")?
        .build()?;

    Ok(channel)
}