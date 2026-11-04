# ABOUT

transmission-remote-gtk is a GTK client for remote management of
the Transmission BitTorrent client, using its HTTP RPC protocol.

It is written in Rust, with [gtk4-rs](https://gtk-rs.org) and libsoup.


# DEPENDENCIES

### Runtime/Build Required Dependencies
The following packages are required dependencies for building and running
transmission-remote-gtk:

 - gtk >= 4.10
 - glib >= 2.70 (including gio)
 - libsoup >= 3.0
 - GNU gettext

An example command for getting the packages required for an Ubuntu/Debian
system:

```
# apt install libgtk-4-dev gettext libsoup-3.0-dev
```

### Optional Dependencies
There are currently no optional dependencies. System tray support used
libappindicator or libayatana-appindicator3, which are only available for
GTK 3, so it is not available in the GTK 4 version.

### Build Only Dependencies
transmission-remote-gtk uses meson for its build system, which builds the
executable with cargo. A relatively new version of meson is required.

 - Rust >= 1.92 and cargo (from your distribution if it's new enough, otherwise
   from [rustup](https://rustup.rs)); the Rust libraries are downloaded by cargo
 - meson >= 0.60.0
 - blueprint-compiler >= 0.16 (the user interface is written in
   [Blueprint](https://gnome.pages.gitlab.gnome.org/blueprint-compiler/); if
   it's missing or too old, meson fetches it with git)
 - appstream (optional, appdata validation using `appstreamcli validate`)
 - desktop-file-utils (optional, desktop file validation)
 - rst2man (optional, manpage generation)

An example of getting the build dependencies for an Ubuntu/Debian system:

```
# apt install gcc cargo meson blueprint-compiler appstream python-docutils
```

**NOTE:** Ubuntu and other stable or LTS distros may have outdated versions of
meson that will not work for compiling transmission-remote-gtk. Meson can also
be installed via pip:

```
# apt install python3-pip
$ pip install meson
```


# BUILDING

Building transmission-remote-gtk is simple. The following commands will clone
the repository, build transmission-remote-gtk, and install it on your system:

```
$ git clone https://github.com/transmission-remote-gtk/transmission-remote-gtk.git
$ cd transmission-remote-gtk
$ meson setup --prefix="$prefix" "$builddir"
$ meson compile -C "$builddir"
# meson install -C "$builddir"
```

For development, cargo can also be used on its own (`cargo build`, `cargo run`,
`cargo test`). Then the translations are compiled into cargo's build directory,
and GTK's data is used from its prefix. `meson devenv -C "$builddir"` gives a
shell with meson's configuration for cargo.

### macOS

On macOS, install the dependencies with [Homebrew](https://brew.sh):

```
$ brew install meson rust blueprint-compiler gtk4 libsoup gettext librsvg adwaita-icon-theme glib-networking
```

Building also creates an application bundle, `"$builddir/Transmission Remote GTK.app"`,
which can be started from the Finder and opens `.torrent` files. It runs the
executable from the build directory and uses the libraries and data installed
by Homebrew (disable it with `-Dmacos_bundle=disabled`). To make it the default
application, link it into `/Applications`:

```
$ ln -s "$PWD/$builddir/Transmission Remote GTK.app" /Applications/
```

A self-contained bundle, which includes all libraries and data and doesn't need
Homebrew, and a disk image containing it can be made with:

```
$ meson compile -C "$builddir" macos-bundle  # $builddir/macos-dist/Transmission Remote GTK.app
$ meson compile -C "$builddir" macos-dmg     # also $builddir/macos-dist/transmission-remote-gtk-<version>-macos-<arch>.dmg
```

It only runs on the macOS version it was built on and newer. It is only signed
ad hoc, so when downloaded, macOS blocks it the first time it is opened; allow
it in System Settings, Privacy & Security.

[Tarball releases](https://github.com/transmission-remote-gtk/transmission-remote-gtk/releases)
are also available to download.


# INSTALLING

Installing transmisison-remote-gtk can be done in a few ways besides building from source. There are many distributions that package transmission-remote-gtk for example. However, there is a cross-distribution Flatpak available that is always up to date.

<a href='https://flathub.org/apps/details/io.github.TransmissionRemoteGtk'><img width='240' alt='Download on Flathub' src='https://flathub.org/assets/badges/flathub-badge-en.png'/></a>

**NOTE**: Only the **latest release** is supported for issues; if your installed version is older you should try the Flatpak or report bugs to your distribution.

## LICENSE

transmission-remote-gtk is released under GNU GPLv2.
See the [COPYING](./COPYING) file for details.

## CONTRIBUTING

This project is under active development, and help is always appreciated regardless of skill level. Current
project goals and refactors are being tracked in [the Wiki](https://github.com/transmission-remote-gtk/transmission-remote-gtk/wiki/TODOs).
Take a look if you're interested in helping. Other PRs are also welcome!

## CONTACT INFO

Development happens on github (this repository). Issues and pull requests should be submitted here. If
you would like to discuss development or ask general questions about the project, you can try `#transmission-remote-gtk`
on [irc.libera.chat](ircs://irc.libera.chat:6697).
