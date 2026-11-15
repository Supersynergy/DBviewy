fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let out =
            std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("open_documents.o");
        let target = std::env::var("TARGET").unwrap();
        let status = std::process::Command::new("xcrun")
            .args([
                "--sdk",
                "macosx",
                "clang",
                "-target",
                &target,
                "-mmacosx-version-min=11.0",
                "-fobjc-arc",
                "-c",
                "pkg/macos/open_documents.m",
                "-o",
            ])
            .arg(&out)
            .status()
            .expect("run macOS SDK compiler");
        assert!(status.success(), "compile Finder document delegate");
        println!("cargo:rerun-if-changed=pkg/macos/open_documents.m");
        println!("cargo:rustc-link-arg-bin=octa={}", out.display());
    }

    // Windows gives the main thread a 1 MB stack, Linux and macOS 8 MB. In a
    // debug build clap's derive for `Cli` alone takes 1.6 MB of it
    // (`augment_args` keeps every argument's builder in its own slot), so the
    // debug binary died before `--version` printed, and every Windows test
    // that runs it with it. A release build needs under 128 KB. Reserve what
    // the other systems give, for the binary only; the GUI's main thread gets
    // the same room.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg-bins=/STACK:8388608");
    }

    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_manifest_file("windows/octa.exe.manifest");
        res.set("ProductName", "Octa");
        res.set("FileDescription", "Multi-format data viewer and editor");
        res.set("LegalCopyright", "MIT License");
        if std::path::Path::new("assets/octa.ico").exists() {
            res.set_icon("assets/octa.ico");
        }
        res.compile().expect("Failed to compile Windows resources");
    }
}
