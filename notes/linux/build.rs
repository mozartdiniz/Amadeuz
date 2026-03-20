/// Build script: on Windows, compile the GResource bundle and embed it in
/// the binary via `gtk::gio::resources_register_include!`.
/// On Linux the Meson build handles resource compilation instead.
fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        glib_build_tools::compile_resources(
            &["data/resources"],
            "data/resources/com.amadeuz.Notes.gresource.xml",
            "amadeuz-notes.gresource",
        );
    }
}
