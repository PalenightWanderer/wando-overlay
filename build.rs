fn main() {
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    println!(
        "cargo:rustc-link-search=native={}",
        root.join("build/Release").display()
    );
    println!("cargo:rustc-link-lib=dylib=overlay-platform");
    println!("cargo:rerun-if-changed=build/Release/overlay-platform.lib");
    // The import library is not rewritten when only the adapter's code changes.
    println!("cargo:rerun-if-changed=build/Release/overlay-platform.dll");
    // Cargo tests and `cargo run` need the SDK DLLs beside the executable.
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let profile = out.ancestors().nth(3).unwrap();
    for entry in std::fs::read_dir(root.join("build/Release")).expect("Run scripts/build.ps1 first")
    {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("dll"))
        {
            let name = path.file_name().unwrap();
            std::fs::copy(&path, profile.join(name)).unwrap();
            std::fs::create_dir_all(profile.join("deps")).unwrap();
            std::fs::copy(&path, profile.join("deps").join(name)).unwrap();
        }
    }
}
