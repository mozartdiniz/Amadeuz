use std::sync::LazyLock;

/// Reads a compile-time environment variable injected by Meson (or build-windows.ps1).
/// Falls back to the provided default when the variable is not set (e.g. plain `cargo build`).
macro_rules! config_var {
    ($name:ident, $default:expr) => {
        pub static $name: LazyLock<&'static str> =
            LazyLock::new(|| option_env!(concat!("MESON_", stringify!($name))).unwrap_or($default));
    };
}

config_var!(APP_ID,  "com.amadeuz.Notes");
config_var!(PATH_ID, "/com/amadeuz/Notes");
config_var!(PKGNAME, "amadeuz-notes");
config_var!(VERSION, env!("CARGO_PKG_VERSION"));
config_var!(PROFILE, "default");
config_var!(DATADIR, "");   // unused on Windows (resources are embedded)
