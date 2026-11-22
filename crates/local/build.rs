use std::process::Command;

/// The engine's Metal code checks the running macOS version before using
/// newer APIs whenever it is built for an older one, as releases are. Those
/// checks call into clang's runtime library, which clang adds to every link
/// it does itself but rustc's link leaves out.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let found = Command::new("xcrun")
        .args(["clang", "--print-runtime-dir"])
        .output()
        .expect("xcrun runs on a Mac with the developer tools");
    assert!(found.status.success(), "xcrun could not find clang");
    let runtime = String::from_utf8(found.stdout).expect("a path");
    println!("cargo:rustc-link-search=native={}", runtime.trim());
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
}
