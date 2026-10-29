fn main() {
    println!("cargo:rerun-if-changed=assets/twitchlite.ico");
    println!("cargo:rerun-if-changed=Cargo.toml");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") { return; }
    winresource::WindowsResource::new()
        .set_icon("assets/twitchlite.ico")
        .set("ProductName", "TwitchLite")
        .set("FileDescription", "TwitchLite")
        .set("CompanyName", "Romanovak")
        .set("InternalName", "TwitchLite")
        .set("OriginalFilename", "twitchlite.exe")
        .set("LegalCopyright", "Copyright (c) 2026 Romanovak")
        .compile().expect("No se pudieron integrar los recursos de TwitchLite");
}
