blueprint_compiler = find_program('blueprint-compiler', required: true)

# Compile Blueprint .blp files → .ui XML
blueprints = custom_target('blueprints',
  input: files(
    'ui/window.blp',
    'ui/auth.blp',
    'ui/note_row.blp',
  ),
  output: '.',
  command: [
    blueprint_compiler, 'batch-compile',
    '@OUTPUT@',
    '@CURRENT_SOURCE_DIR@',
    '@INPUT@'
  ],
)

# Bundle UI files into a GResource
gnome.compile_resources(
  app_id,
  'com.amadeuz.Notes.gresource.xml',
  gresource_bundle: true,
  source_dir: meson.current_build_dir(),
  install: true,
  install_dir: datadir / pkgname,
  dependencies: blueprints,
)

# Install GSettings schema
install_data(
  'com.amadeuz.Notes.gschema.xml',
  install_dir: datadir / 'glib-2.0' / 'schemas',
)

# Desktop entry (makes the app appear in GNOME Shell / app grid)
install_data(
  'com.amadeuz.Notes.desktop',
  install_dir: datadir / 'applications',
)

# AppStream metadata
install_data(
  'com.amadeuz.Notes.metainfo.xml',
  install_dir: datadir / 'metainfo',
)

install_data(
  'icons/hicolor/256x256/apps/com.amadeuz.Notes.png',
  install_dir: datadir / 'icons' / 'hicolor' / '256x256' / 'apps',
)

gnome.post_install(
  glib_compile_schemas: true,
  gtk_update_icon_cache: true,
)
