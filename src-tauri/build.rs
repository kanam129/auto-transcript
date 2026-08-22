fn main() {
    delay_load_vulkan();
    tauri_build::build()
}

/// Makes `vulkan-1.dll` load on first use rather than at process start.
///
/// Linking Vulkan the ordinary way puts a hard dependency in the executable's import
/// table, so on a machine with no Vulkan driver Windows refuses to start the program at
/// all — before any of our code runs, with a dialog that explains nothing. Delay-loading
/// turns that into something we can handle: `stt::engine` checks whether the library
/// exists and quietly stays on the CPU when it does not.
///
/// Only relevant to the MSVC toolchain, and only when the Vulkan backend is compiled in.
fn delay_load_vulkan() {
    let vulkan = std::env::var("CARGO_FEATURE_GPU_VULKAN").is_ok();
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    if !(vulkan && msvc && windows) {
        return;
    }
    // delayimp.lib holds the helper that resolves the import on the first call.
    println!("cargo:rustc-link-lib=dylib=delayimp");
    println!("cargo:rustc-link-arg=/DELAYLOAD:vulkan-1.dll");
}
