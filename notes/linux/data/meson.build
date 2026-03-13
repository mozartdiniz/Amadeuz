blueprint_compiler = find_program('blueprint-compiler', required: true)

# Compile Blueprint .blp files → .ui XML
blueprints = custom_target('blueprints',
  input: files('ui/window.blp'),
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

gnome.post_install(glib_compile_schemas: true)
