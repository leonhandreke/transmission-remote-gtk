src_files = files(
  'bencode.c',
  'hig.c',
  'icons.c',
  'json.c',
  'main.c',
  'requests.c',
  'session-get.c',
  'torrent-row.c',
  'torrent.c',
  'trg-about-window.c',
  'trg-client.c',
  'trg-destination-combo.c',
  'trg-dialog.c',
  'trg-file-parser.c',
  'trg-file-rename-dialog.c',
  'trg-files-model.c',
  'trg-files-tree-view.c',
  'trg-files-tree.c',
  'trg-general-panel.c',
  'trg-gtk-app.c',
  'trg-item.c',
  'trg-json-widgets.c',
  'trg-main-window.c',
  'trg-menu-bar.c',
  'trg-model.c',
  'trg-peers-model.c',
  'trg-peers-tree-view.c',
  'trg-persistent-tree-view.c',
  'trg-preferences-dialog.c',
  'trg-prefs.c',
  'trg-remote-prefs-dialog.c',
  'trg-state-selector.c',
  'trg-stats-dialog.c',
  'trg-status-bar.c',
  'trg-toolbar.c',
  'trg-torrent-add-dialog.c',
  'trg-torrent-add-url-dialog.c',
  'trg-torrent-model.c',
  'trg-torrent-move-dialog.c',
  'trg-torrent-props-dialog.c',
  'trg-torrent-tree-view.c',
  'trg-trackers-model.c',
  'trg-trackers-tree-view.c',
  'trg-tree-view.c',
  'upload.c',
  'util.c',
)

# UI definitions, compiled from Blueprint to GtkBuilder XML. batch-compile
# writes each foo.blp to foo.ui in the build directory, where the resources
# below pick them up.
blueprint_compiler = find_program('blueprint-compiler', version: '>= 0.16')

blueprint_files = files(
  'trg-file-rename-dialog.blp',
  'trg-general-panel.blp',
  'trg-status-bar.blp',
  'trg-toolbar.blp',
  'trg-torrent-add-url-dialog.blp',
  'trg-torrent-move-dialog.blp',
)

blueprints = custom_target(
  'blueprints',
  input: blueprint_files,
  output: '.',
  command: [blueprint_compiler, 'batch-compile', '@OUTPUT@', '@CURRENT_SOURCE_DIR@', '@INPUT@'],
)

ui_resources = gnome.compile_resources(
  'trg-ui-resources',
  'trg.gresource.xml',
  source_dir: meson.current_build_dir(),
  dependencies: blueprints,
)

# generate config file
configure_file(output: 'config.h', configuration: conf_data)

# Add gresources
src_files += gresources
src_files += ui_resources

trg_exe = executable(
  project_name,
  sources: src_files,
  dependencies: trg_deps,
  link_depends: trg_link_depends,
  install: true,
  pie: true,
)