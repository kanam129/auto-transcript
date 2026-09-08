fn main() {
    delay_load_vulkan();
    link_compiler_rt();
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

/// Links clang's compiler runtime so `@available` checks inside whisper.cpp resolve.
///
/// `ggml-metal-device.m` guards Metal residency sets behind `@available(macOS 15.0, *)`.
/// When the deployment target is lower than that — and ours is 13.0, from
/// `bundle.macOS.minimumSystemVersion` — clang cannot fold the check away, so it emits a
/// call to `__isPlatformVersionAtLeast`. That symbol lives in `libclang_rt.osx.a`, which
/// clang normally links itself but rustc suppresses with `-nodefaultlibs`; the link then
/// fails with "Undefined symbols for architecture arm64".
///
/// Only `cargo build --target ...` hits this. A plain host build inherits the running
/// macOS version as its deployment target, which is >= 15.0 on any machine new enough to
/// notice, so the check folds to a constant and the symbol is never referenced. That is
/// why CI stayed green while the release workflow — which passes `--target` to produce
/// both Apple silicon and Intel builds — failed on macOS.
fn link_compiler_rt() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    // The path moves with every Xcode release, so ask the toolchain rather than guess.
    let Ok(out) = std::process::Command::new("clang")
        .arg("--print-runtime-dir")
        .output()
    else {
        return;
    };
    let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
    // The archive is universal (arm64 + x86_64), so one search path serves both targets.
    if !out.status.success() || !std::path::Path::new(&dir).join("libclang_rt.osx.a").exists() {
        return;
    }
    println!("cargo:rustc-link-search=native={dir}");
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
}
