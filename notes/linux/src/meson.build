cargo = find_program('cargo', required: true)

profile      = get_option('profile')
cross_target = get_option('target')
is_devel     = profile == 'development'

if is_devel
  rust_target = 'debug'
  cargo_args  = []
else
  rust_target = 'release'
  cargo_args  = ['--release']
endif

bin_name        = 'amadeuz-notes'
bin_suffix      = ''
bin_install_dir = bindir

if cross_target != ''
  if cross_target.contains('windows')
    bin_suffix      = '.exe'
    bin_install_dir = ''
  endif
endif

cargo_env = environment()
cargo_env.set('CARGO_HOME',    meson.project_build_root() / 'cargo-home')
cargo_env.set('MESON_APP_ID',  app_id)
cargo_env.set('MESON_PATH_ID', path_id)
cargo_env.set('MESON_PKGNAME', pkgname)
cargo_env.set('MESON_VERSION', meson.project_version())
cargo_env.set('MESON_PROFILE', profile)
cargo_env.set('MESON_DATADIR', datadir)

custom_target(
  'cargo-build',
  build_by_default:  true,
  build_always_stale: true,
  output:            bin_name + bin_suffix,
  console:           true,
  install:           true,
  install_dir:       bin_install_dir,
  env:               cargo_env,
  command: [
    find_program(meson.project_source_root() / 'build-aux' / 'meson' / 'cargo_wrapper.py'),
    '--manifest-path', meson.project_source_root() / 'Cargo.toml',
    '--bin',           bin_name,
    '--target-dir',    meson.project_build_root() / 'target',
    cross_target != '' ? ['--target', cross_target] : [],
    cargo_args,
    '@OUTPUT@',
  ],
)
