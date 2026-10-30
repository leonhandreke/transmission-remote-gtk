project(
  'transmission-remote-gtk',
  'c',
  license: 'GPL2+',
  version: '1.7.1',
  meson_version: '>= 0.60.0',
  default_options: ['b_pie=true', 'c_std=gnu11', 'warning_level=2', 'buildtype=debugoptimized'],
)

project_name = meson.project_name()
project_version = meson.project_version()
app_id = 'io.github.TransmissionRemoteGtk'
resource_prefix = '/' + app_id.replace('.', '/')
pretty_name = 'Transmission Remote GTK'
datadir = get_option('datadir')

# glib version info
glib_version = '2.70'
glib_min_version = glib_version
glib_version_str = '>= @0@'.format(glib_version)

# glib API configuration
maj_min = glib_min_version.split('.')
glib_min = 'GLIB_VERSION_@0@_@1@'.format(maj_min[0], maj_min[1])
glib_min_def = '-DGLIB_VERSION_MIN_REQUIRED=@0@'.format(glib_min)

add_project_arguments(glib_min_def, language: 'c')

# required dependencies
gtk_dep = dependency('gtk4', version: '>= 4.10')
gio_dep = dependency('gio-2.0', version: glib_version_str)
glib_dep = dependency('glib-2.0', version: glib_version_str)
json_dep = dependency('json-glib-1.0', version: '>= 1.2.8')
libsoup_dep = dependency('libsoup-3.0', version: '>= 3.0')
gthread_dep = dependency('gthread-2.0', version: glib_version_str)

trg_deps = [gtk_dep, glib_dep, gio_dep, json_dep, libsoup_dep, gthread_dep]

# optional dependencies
# libappindicator only exists for GTK 3 and cannot be used together with GTK 4.
if get_option('libappindicator').enabled()
  error('libappindicator (system tray) support is not available with GTK 4')
endif

# compiler
cc = meson.get_compiler('c')

# compiler deps
trg_deps += cc.find_library('m', required: false)

# macOS: following the system's dark mode uses CoreFoundation directly
if host_machine.system() == 'darwin'
  trg_deps += dependency('appleframeworks', modules: ['CoreFoundation'])
endif

# nl_langinfo check
nl_langinfo = cc.has_function('nl_langinfo', prefix: '#include <langinfo.h>')
nl_li_feature = get_option('nl_langinfo')
if nl_li_feature.enabled() and not nl_langinfo
  error('nl_langinfo() specified but not found')
endif
nl_langinfo = nl_langinfo and nl_li_feature.allowed()

# config data
conf_data = configuration_data()

conf_data.set_quoted('PACKAGE_NAME', project_name)
conf_data.set_quoted('PACKAGE_VERSION', project_version)
conf_data.set_quoted('GETTEXT_PACKAGE', project_name)
conf_data.set_quoted('APPLICATION_ID', app_id)
conf_data.set_quoted('LOCALEDIR', get_option('prefix') / get_option('localedir'))
if host_machine.system() == 'darwin'
  conf_data.set_quoted('DATADIR', get_option('prefix') / datadir)
endif

# debug options
debug = get_option('debug')
conf_data.set('DEBUG', debug)
conf_data.set('G_ENABLE_DEBUG', debug)
conf_data.set('G_DISABLE_ASSERT', not debug)
conf_data.set('G_DISABLE_CHECKS', not debug)
conf_data.set('G_DISABLE_CAST_CHECKS', not debug)

# compile time features
conf_data.set10('ENABLE_NL_LANGINFO', nl_langinfo)

# compiler/linker flags
flags = [
  '-funsigned-char',
  '-fstack-protector-strong',
  '-Winline',
  '-Wno-unused-parameter',
  '-Wstrict-prototypes',
  '-Wmissing-prototypes',
  '-Werror=format=2',
  '-Werror=implicit-function-declaration',
  '-Werror=pointer-arith',
  '-Werror=init-self',
  '-Werror=missing-include-dirs',
  '-Werror=date-time',
]

cflags = cc.get_supported_arguments(flags)
add_project_arguments(cflags, language: 'c')

# link arguments
ldflags = cc.get_supported_link_arguments('-Wl,-z,relro', '-Wl,-z,now')
add_project_link_arguments(ldflags, language: 'c')

trg_link_depends = []

# macOS: embed the Info.plist (also used for an application bundle) in the
# executable, so it has the right name in the application menu even when it
# isn't run from a bundle.
if host_machine.system() == 'darwin'
  info_plist = configure_file(
    input: 'macos/Info.plist.in',
    output: 'Info.plist',
    configuration: {
      'PRETTY_NAME': pretty_name,
      'APP_ID': app_id,
      'PROJECT_NAME': project_name,
      'VERSION': project_version,
    },
  )
  add_project_link_arguments(
    '-Wl,-sectcreate,__TEXT,__info_plist,' + meson.current_build_dir() / 'Info.plist',
    language: 'c',
  )
  trg_link_depends += info_plist
endif

# Windows compiler options
if host_machine.system() == 'windows' or host_machine.system() == 'cygwin'
  win_cflags = cc.get_supported_arguments('-mms-bitfields', '-mwin32', '-mwindows')
  add_project_arguments(win_cflags, language: 'c')

  win_ldflags = cc.get_supported_link_arguments('-Wl,--allow-multiple-definition', '-lws2_32', '-lintl', '-lssp')
  add_project_link_arguments(win_ldflags, language: 'c')
endif

subdir('po')
subdir('data')
subdir('src')

# macOS application bundle
macos_bundle = get_option('macos_bundle').require(
  host_machine.system() == 'darwin',
  error_message: 'macOS application bundles can only be built on macOS',
)
if macos_bundle.allowed()
  python = import('python').find_installation()
  rsvg_convert = find_program('rsvg-convert', required: macos_bundle)
  iconutil = find_program('iconutil', required: macos_bundle)
  macos_bundle = macos_bundle.require(
    rsvg_convert.found() and iconutil.found(),
    error_message: 'rsvg-convert (librsvg) and iconutil are needed for the macOS application bundle',
  )
endif
if macos_bundle.allowed()
  # A bundle for development, which runs the executable from this build and
  # uses the data of the prefix (usually Homebrew).
  bundle_py = files('macos/bundle.py')
  app_svg = files('data/icons/@0@.svg'.format(project_name))
  app_name = pretty_name + '.app'

  app_icns = custom_target(
    'macos-icns',
    input: app_svg,
    output: project_name + '.icns',
    command: [python, bundle_py, 'icns', rsvg_convert, iconutil, '@INPUT@', '@OUTPUT@'],
  )

  app_bundle = custom_target(
    'macos-app',
    input: [trg_exe, info_plist, app_icns, app_svg],
    output: app_name,
    command: [python, bundle_py, 'app', '@INPUT@', '@OUTPUT@'],
    build_by_default: true,
  )

  # A self-contained, relocatable bundle including the libraries and data,
  # for distribution: 'meson compile macos-bundle' or 'meson compile macos-dmg'
  dist_dir = meson.current_build_dir() / 'macos-dist'
  arch = host_machine.cpu_family() == 'aarch64' ? 'arm64' : host_machine.cpu_family()
  dmg_name = '@0@-@1@-macos-@2@.dmg'.format(project_name, project_version, arch)
  standalone_cmd = [
    python,
    bundle_py,
    'standalone',
    '--po-dir', meson.current_build_dir() / 'po',
    app_bundle,
    dist_dir / app_name,
  ]
  standalone_deps = [app_bundle, po_targets[0]]
  run_target('macos-bundle', command: standalone_cmd, depends: standalone_deps)
  run_target(
    'macos-dmg',
    command: standalone_cmd + ['--dmg', dist_dir / dmg_name],
    depends: standalone_deps,
  )
endif
