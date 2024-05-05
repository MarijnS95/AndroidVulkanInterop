use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bindings = bindgen::builder()
        .header("../AHardwareBufferHelpers.h")
        .clang_arg("-xc++")
        // Skip types that we don't specify, we define them from Rust code
        .allowlist_recursively(false)
        .allowlist_type("android::GraphicBuffer")
        .allowlist_function("android::\\w+")
        // .allowlist_type("SomeCoolClass")
        // .allowlist_function("do_some_cool_thing")
        .generate()?;

    bindings.write_to_file(
        PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("AHardwareBufferHelpers.rs"),
    )?;

    Ok(())
}
