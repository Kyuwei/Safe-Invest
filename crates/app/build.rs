fn main() {
    // Embeds the frontend, the Windows manifest and the icon. Skipped for the
    // console-only build so the CLI still compiles on a machine with no webview.
    #[cfg(feature = "gui")]
    tauri_build::build();
}
