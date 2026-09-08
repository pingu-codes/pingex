fn main() {
    tauri_build::build();
    // rfd imports TaskDialogIndirect, which exists only in common-controls
    // v6. The app has a manifest, but Cargo's test executables need one too.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
        // Tauri already embeds the application's full manifest in resource.lib.
        println!("cargo:rustc-link-arg-bin=pingex-app=/MANIFEST:NO");
    }
}
