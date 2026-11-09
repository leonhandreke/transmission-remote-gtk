project(
  'transmission-remote-gtk',
  license: 'GPL2+',
  version: '1.7.1',
  meson_version: '>= 0.60.0',
  default_options: ['buildtype=debugoptimized'],
)

project_name = meson.project_name()
project_version = meson.project_version()
app_id = 'io.github.TransmissionRemoteGtk'
pretty_name = 'Transmission Remote'
prefix = get_option('prefix')
datadir = get_option('datadir')

# The application is written in Rust and built with cargo (see Cargo.toml and
# build.rs). The dependencies are checked here too, so that a missing one is
# reported by meson.
dependency('gtk4', version: '>= 4.12')
dependency('glib-2.0', version: '>= 2.70')
dependency('gio-2.0', version: '>= 2.70')
dependency('libsoup-3.0', version: '>= 3.0')

cargo = find_program('cargo')
# The user interface is written in Blueprint, which build.rs compiles. If
# it's missing or too old, meson fetches it with git.
blueprint_compiler = find_program('blueprint-compiler', version: '>= 0.16')
python = import('python').find_installation()

# libappindicator only exists for GTK 3 and cannot be used together with GTK 4.
if get_option('libappindicator').enabled()
  error('libappindicator (system tray) support is not available with GTK 4')
endif

cargo_args = []
if not get_option('nl_langinfo').allowed()
  cargo_args += '--no-default-features'
endif

profile = get_option('buildtype') == 'debug' ? 'debug' : 'release'
cargo_target_dir = meson.project_build_root() / 'target'

cargo_env = environment()
cargo_env.set('TRG_LOCALEDIR', prefix / get_option('localedir'))
cargo_env.set('TRG_DATADIR', prefix / datadir)
cargo_env.set('BLUEPRINT_COMPILER', blueprint_compiler.full_path())
cargo_env.set('CARGO', cargo.full_path())
# `meson devenv` gives a shell with this environment, where plain cargo works
meson.add_devenv(cargo_env)

trg_exe = custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  console: true,
  output: project_name,
  env: cargo_env,
  command: [
    python,
    files('build-aux/cargo_build.py'),
    meson.project_source_root(),
    cargo_target_dir,
    profile,
    '@OUTPUT@',
    cargo_args,
  ],
  install: true,
  install_dir: get_option('bindir'),
)

# The Info.plist (also embedded in the executable by build.rs)
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
endif

test('cargo test', cargo, args: ['test', '--manifest-path', meson.project_source_root() / 'Cargo.toml', '--target-dir', cargo_target_dir], env: cargo_env, timeout: 600)

subdir('po')
subdir('data')

# macOS application bundle
macos_bundle = get_option('macos_bundle').require(
  host_machine.system() == 'darwin',
  error_message: 'macOS application bundles can only be built on macOS',
)
if macos_bundle.allowed()
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
