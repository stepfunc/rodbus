use oo_bindgen::model::*;

const NOTHING: &str = "nothing";

pub(crate) fn define(lib: &mut LibraryBuilder) -> BackTraced<UniversalStructHandle> {
    let app_decode_level_enum = lib
        .define_enum("app_decode_level")?
        .push(NOTHING, "Decode nothing")?
        .push("function_code", "Decode the function code only")?
        .push("data_headers", "Decode the function code and the general description of the data")?
        .push(
            "data_values",
            "Decode the function code, the general description of the data and the actual data values",
        )?
        .doc(
            doc("Controls how transmitted and received message at the application layer are decoded at the INFO log level")
                .details("Application-layer messages are referred to as Protocol Data Units (PDUs) in the specification.")
        )?
        .build()?;

    let frame_decode_level_enum = lib
        .define_enum("frame_decode_level")?
        .push(NOTHING, "Log nothing")?
        .push("header", " Decode the header")?
        .push("payload", "Decode the header and the raw payload as hexadecimal")?
        .doc(
            doc("Controls how the transmitted and received frames are decoded at the INFO log level")
                .details("Transport-specific framing wraps the application-layer traffic. You'll see these frames called ADUs in the Modbus specification.")
                .details("On TCP, this is the MBAP decoding. On serial, this controls the serial line PDU.")
        )?
        .build()?;

    let phys_decode_level_enum = lib
        .define_enum("phys_decode_level")?
        .push(NOTHING, "Log nothing")?
        .push(
            "length",
            "Log only the length of data that is sent and received",
        )?
        .push(
            "data",
            "Log the length and the actual data that is sent and received",
        )?
        .doc("Controls how data transmitted at the physical layer (TCP, serial, etc) is logged")?
        .build()?;

    let app_field = Name::create("app")?;
    let frame_field = Name::create("frame")?;
    let physical_field = Name::create("physical")?;

    let decode_level_struct = lib.declare_universal_struct("decode_level")?;
    let decode_level_struct = lib.define_universal_struct(decode_level_struct)?
        .add(&app_field, app_decode_level_enum, "Controls decoding of the application layer (PDU)")?
        .add(&frame_field, frame_decode_level_enum, "Controls decoding of frames (MBAP / Serial PDU)")?
        .add(&physical_field, phys_decode_level_enum, "Controls the logging of physical layer read/write")?
        .doc("Controls the decoding of transmitted and received data at the application, frame, and physical layer")?
        .end_fields()?
        .add_full_initializer("build")?
        .begin_initializer("nothing", InitializerType::Static, "Initialize log levels to defaults which is to decode nothing")?
        .default_variant(&app_field, NOTHING)?
        .default_variant(&frame_field, NOTHING)?
        .default_variant(&physical_field, NOTHING)?
        .end_initializer()?
        .build()?;

    Ok(decode_level_struct)
}
