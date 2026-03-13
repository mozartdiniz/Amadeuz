cargo = find_program('cargo')

cargo_env = [
  'CARGO_HOME='    + meson.project_build_root() / 'cargo-home',
  'MESON_APP_ID='  + app_id,
  'MESON_PATH_ID=' + path_id,
  'MESON_PKGNAME=' + pkgname,
  'MESON_VERSION=' + meson.project_version(),
  'MESON_PROFILE=' + profile,
  'MESON_DATADIR=' + datadir,
]

cargo_options = [
  '--manifest-path', meson.project_source_root() / 'Cargo.toml',
  '--target-dir',    meson.project_build_root() / 'target',
]

if profile == 'default'
  cargo_options += ['--release']
  rust_target = 'release'
  message('Building in release mode')
else
  rust_target = 'debug'
  message('Building in debug mode')
endif

custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: pkgname,
  console: true,
  install: true,
  install_dir: bindir,
  command: [
    'env', cargo_env,
    cargo, 'build', cargo_options,
    '&&',
    'cp',
    meson.project_build_root() / 'target' / rust_target / 'amadeuz-notes',
    '@OUTPUT@',
  ],
)
