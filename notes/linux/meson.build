project('amadeuz-notes', 'rust',
  version: '0.1.0',
  meson_version: '>= 1.0.0',
  license: 'MIT',
)

gnome = import('gnome')

dependency('glib-2.0',      version: '>= 2.74')
dependency('gio-2.0',       version: '>= 2.74')
dependency('gtk4',          version: '>= 4.12')
dependency('libadwaita-1',  version: '>= 1.4')

app_id  = 'com.amadeuz.Notes'
path_id = '/com/amadeuz/Notes'
pkgname = meson.project_name()
profile = get_option('profile')
datadir = get_option('prefix') / get_option('datadir')
bindir  = get_option('prefix') / get_option('bindir')

if profile == 'development'
  app_id = app_id + '.Devel'
endif

subdir('data')
subdir('src')
