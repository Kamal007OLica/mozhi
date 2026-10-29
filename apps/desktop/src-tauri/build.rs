fn main() {
    // whisper.cpp's Metal backend uses `@available(...)`, which clang compiles into calls to
    // `__isPlatformVersionAtLeast` from its runtime library. rustc links with -nodefaultlibs,
    // so with newer Xcode SDKs that symbol goes missing at link time. Link the runtime
    // explicitly; the linker only pulls what's undefined, so this is a no-op elsewhere.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        if let Ok(out) = std::process::Command::new("xcrun").args(["clang", "--print-resource-dir"]).output() {
            let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let lib = std::path::Path::new(&dir).join("lib/darwin/libclang_rt.osx.a");
            if lib.is_file() {
                println!("cargo:rustc-link-arg={}", lib.display());
            }
        }
    }
    tauri_build::build()
}
