use std::sync::LazyLock;

/// Reads a compile-time environment variable injected by Meson via `cargo_env`.
/// Panics on first access if the variable was not set at build time.
macro_rules! config_var {
    ($name:ident) => {
        #[expect(clippy::option_env_unwrap)]
        pub static $name: LazyLock<&'static str> = LazyLock::new(|| {
            option_env!(concat!("MESON_", stringify!($name))).expect(concat!(
                "MESON_",
                stringify!($name),
                " was not set at compile time. Build via Meson, not plain `cargo build`."
            ))
        });
    };
}

config_var!(APP_ID);
config_var!(PATH_ID);
config_var!(PKGNAME);
config_var!(VERSION);
config_var!(PROFILE);
config_var!(DATADIR);
