//! Embeds the app icon in the Windows executable, so `bia.exe` shows the
//! hummingbird in Explorer, the taskbar and the Start menu. Other platforms
//! get their icon from the app bundle (macOS) or the .desktop file (Linux).

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icons/bia.ico");

    #[cfg(windows)]
    {
        let icon = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/icons/bia.ico");
        winresource::WindowsResource::new()
            .set_icon(icon)
            .compile()
            .expect("embedding the Windows icon");
    }
}
