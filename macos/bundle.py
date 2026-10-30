#!/usr/bin/env python3
"""Create macOS application bundles for transmission-remote-gtk.

Called from meson.build:

  icns RSVG_CONVERT ICONUTIL SVG OUTPUT
      Render the application icon into an .icns file.

  app EXECUTABLE INFO_PLIST ICNS SVG OUTPUT
      Create an application bundle for development. It contains a copy of
      the executable, which uses the libraries and data of the prefix its
      dependencies are installed in (usually Homebrew).

  standalone [--po-dir DIR] [--deps-prefix DIR] [--dmg DMG] APP OUTPUT
      Turn the development bundle APP into a self-contained, relocatable
      bundle: copy all non-system libraries into Contents/Frameworks and the
      runtime data GTK needs into Contents/Resources, make the libraries
      refer to each other through @rpath and sign the bundle ad hoc.
      Optionally also create a disk image.

The executable finds the bundled data relative to itself on startup (see
macos_setup_environment() in src/main.c).
"""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ICON_SIZES = [16, 32, 128, 256, 512]

# GSettings schemas used by GTK itself
GTK_SCHEMAS = "org.gtk.gtk4.Settings.*.gschema.xml"

# GTK's message catalog, shipped for the languages we have (see src/main.c)
LIBRARY_DOMAINS = ["gtk40"]

PIXBUF_DIR = "lib/gdk-pixbuf-2.0/2.10.0"
GIO_MODULES_DIR = "lib/gio/modules"


def run(*cmd, **kwargs):
    return subprocess.run([str(c) for c in cmd], check=True, **kwargs)


def output(*cmd):
    return run(*cmd, stdout=subprocess.PIPE, text=True).stdout


def run_filtered(*cmd, hide):
    """Run a command, hiding expected messages containing hide."""
    cmd = [str(c) for c in cmd]
    result = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    for line in result.stdout.splitlines():
        if hide not in line:
            print(line, file=sys.stderr)
    if result.returncode != 0:
        fail(f"{' '.join(cmd)} failed")


def install_name_tool(*args):
    # The code is signed again at the end
    run_filtered("install_name_tool", *args, hide="invalidate the code signature")


def codesign(path):
    run_filtered("codesign", "--force", "--sign", "-", path, hide="replacing existing signature")


def fail(msg):
    sys.exit(f"bundle.py: error: {msg}")


def make_icns(args):
    with tempfile.TemporaryDirectory() as tmp:
        iconset = Path(tmp) / "icon.iconset"
        iconset.mkdir()
        for size in ICON_SIZES:
            for scale in (1, 2):
                suffix = "@2x" if scale == 2 else ""
                png = iconset / f"icon_{size}x{size}{suffix}.png"
                px = size * scale
                run(args.rsvg_convert, "-w", px, "-h", px, "-o", png, args.svg)
        run(args.iconutil, "-c", "icns", "-o", args.output, iconset)


def make_app(args):
    app = Path(args.output)
    contents = app / "Contents"
    macos = contents / "MacOS"
    resources = contents / "Resources"
    icons = resources / "share" / "icons" / "hicolor" / "scalable" / "apps"
    for d in (macos, icons):
        d.mkdir(parents=True, exist_ok=True)

    exe = Path(args.executable)
    # Replace the executable rather than overwriting it, as macOS kills
    # processes whose (signed) executable changed in place.
    tmp = macos / f".{exe.name}.tmp"
    shutil.copy2(exe, tmp)
    os.replace(tmp, macos / exe.name)

    shutil.copy2(args.info_plist, contents / "Info.plist")
    shutil.copy2(args.icns, resources)
    # The application icon for the about dialog and notifications
    shutil.copy2(args.svg, icons)
    (contents / "PkgInfo").write_text("APPL????")

    # Let ninja know the bundle is up to date.
    os.utime(app)


def macho_files(root):
    """All Mach-O files below root (not following symlinks)."""
    for path in sorted(Path(root).rglob("*")):
        if path.is_symlink() or not path.is_file():
            continue
        with open(path, "rb") as f:
            magic = f.read(4)
        if magic in (b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe"):
            yield path


def load_commands(path):
    """Return (install names of linked libraries, rpaths) of a Mach-O file."""
    libs, rpaths = [], []
    lines = output("otool", "-l", path).splitlines()
    for i, line in enumerate(lines):
        cmd = line.strip()
        if cmd in ("cmd LC_LOAD_DYLIB", "cmd LC_LOAD_WEAK_DYLIB", "cmd LC_REEXPORT_DYLIB"):
            key = "name"
        elif cmd == "cmd LC_RPATH":
            key = "path"
        else:
            continue
        for following in lines[i + 1 : i + 4]:
            fields = following.split()
            if fields and fields[0] == key:
                value = following.split(None, 1)[1].rsplit(" (offset", 1)[0]
                (libs if key == "name" else rpaths).append(value)
                break
    return libs, rpaths


def is_system(name):
    return name.startswith(("/usr/lib/", "/System/"))


class Bundler:
    def __init__(self, app, deps_prefix):
        self.app = Path(app)
        self.contents = self.app / "Contents"
        self.frameworks = self.contents / "Frameworks"
        self.resources = self.contents / "Resources"
        self.prefix = Path(deps_prefix)
        # basename -> original path of the libraries copied to Frameworks
        self.libraries = {}

    def resolve(self, name, origin, rpaths, exe_dir):
        """Find the file an install name refers to."""
        loader_dir = str(origin.resolve().parent)
        if name.startswith("@rpath/"):
            rest = name[len("@rpath/") :]
            for rpath in rpaths:
                rpath = rpath.replace("@loader_path", loader_dir)
                rpath = rpath.replace("@executable_path", exe_dir)
                candidate = Path(rpath) / rest
                if candidate.exists():
                    return candidate
            fail(f"cannot resolve {name} needed by {origin}")
        name = name.replace("@loader_path", loader_dir)
        name = name.replace("@executable_path", exe_dir)
        if not Path(name).exists():
            fail(f"{name} needed by {origin} does not exist")
        return Path(name)

    def collect(self, binary, origin, exe_dir, inherited_rpaths=()):
        """Copy the libraries binary needs into Frameworks, recursively.

        origin is the binary's original location, for resolving
        @loader_path. Returns the changes to make to its install names.
        """
        libs, rpaths = load_commands(binary)
        search = list(rpaths) + list(inherited_rpaths)
        changes = {}
        for name in libs:
            if is_system(name):
                continue
            src = self.resolve(name, origin, search, exe_dir)
            base = Path(name).name
            changes[name] = f"@rpath/{base}"
            known = self.libraries.get(base)
            if known is not None:
                if known.resolve() != src.resolve():
                    fail(f"two different libraries named {base}: {known}, {src}")
                continue
            self.libraries[base] = src
            dest = self.frameworks / base
            shutil.copy2(src.resolve(), dest)
            dest.chmod(0o755)
            self.fix(dest, src, exe_dir, "@loader_path", search, dylib_id=f"@rpath/{base}")
        return changes

    def fix(self, binary, origin, exe_dir, rpath, inherited_rpaths=(), dylib_id=None):
        """Bundle the dependencies of binary and point it at them."""
        changes = self.collect(binary, origin, exe_dir, inherited_rpaths)
        args = []
        for old, new in changes.items():
            args += ["-change", old, new]
        if dylib_id:
            args += ["-id", dylib_id]
        if args:
            install_name_tool(*args, binary)
        # Remove all rpaths one by one, as they may be duplicated
        while True:
            rpaths = load_commands(binary)[1]
            if not rpaths:
                break
            install_name_tool("-delete_rpath", rpaths[0], binary)
        install_name_tool("-add_rpath", rpath, binary)

    def add_module(self, src, dest_dir):
        """Copy a loadable module (gdk-pixbuf loader, GIO module) into the bundle."""
        dest_dir.mkdir(parents=True, exist_ok=True)
        dest = dest_dir / src.name
        shutil.copy2(src.resolve(), dest)
        dest.chmod(0o755)
        rpath = "@loader_path/" + os.path.relpath(self.frameworks, dest_dir)
        self.fix(dest, src, str(self.prefix / "bin"), rpath)
        return dest

    def bundle_libraries(self):
        exe_dir = self.contents / "MacOS"
        self.frameworks.mkdir(exist_ok=True)
        for exe in macho_files(exe_dir):
            # Its rpaths and install names point into the build directory
            # and the dependencies' prefix.
            self.fix(exe, exe, str(exe_dir), "@executable_path/../Frameworks")

    def bundle_pixbuf_loaders(self):
        # PNG and JPEG are built into gdk-pixbuf, add SVG (used for icons)
        src_dir = self.prefix / PIXBUF_DIR / "loaders"
        loaders = sorted(src_dir.glob("libpixbufloader*svg*.so"))
        if not loaders:
            fail(f"SVG gdk-pixbuf loader not found in {src_dir}")
        dest_dir = self.resources / PIXBUF_DIR / "loaders"
        for loader in loaders:
            self.add_module(loader, dest_dir)
        # Like gtk-mac-bundler, refer to the loaders relative to the
        # executable, which dlopen() understands.
        cache = output(self.prefix / "bin" / "gdk-pixbuf-query-loaders", *loaders)
        cache = cache.replace(
            f'"{src_dir}/', f'"@executable_path/../Resources/{PIXBUF_DIR}/loaders/'
        )
        cache = "\n".join(line for line in cache.splitlines() if not line.startswith("#"))
        (self.resources / PIXBUF_DIR / "loaders.cache").write_text(cache + "\n")

    def bundle_gio_modules(self):
        # TLS support (glib-networking), needed for HTTPS connections
        src_dir = self.prefix / GIO_MODULES_DIR
        modules = sorted(src_dir.glob("libgio*tls*.so"))
        if not modules:
            print(f"warning: no TLS GIO module in {src_dir}, HTTPS will not work")
        for module in modules:
            self.add_module(module, self.resources / GIO_MODULES_DIR)

    def bundle_schemas(self):
        dest = self.resources / "share" / "glib-2.0" / "schemas"
        dest.mkdir(parents=True, exist_ok=True)
        schemas = sorted((self.prefix / "share" / "glib-2.0" / "schemas").glob(GTK_SCHEMAS))
        if not schemas:
            fail("GTK's GSettings schemas not found")
        for schema in schemas:
            shutil.copy2(schema, dest)
        run(self.prefix / "bin" / "glib-compile-schemas", dest)

    def bundle_icons(self):
        icons = self.resources / "share" / "icons"
        shutil.copytree(self.prefix / "share" / "icons" / "Adwaita", icons / "Adwaita",
                        symlinks=False, dirs_exist_ok=True,
                        ignore=shutil.ignore_patterns("cursors"))
        shutil.copy2(self.prefix / "share" / "icons" / "hicolor" / "index.theme",
                     icons / "hicolor")
        for theme in ("Adwaita", "hicolor"):
            run(self.prefix / "bin" / "gtk4-update-icon-cache", "--quiet", "--force", icons / theme)

    def bundle_locales(self, po_dir, domain):
        dest = self.resources / "share" / "locale"
        for mo in sorted(Path(po_dir).glob(f"*/LC_MESSAGES/{domain}.mo")):
            lang = mo.parent.parent.name
            lc_messages = dest / lang / "LC_MESSAGES"
            lc_messages.mkdir(parents=True, exist_ok=True)
            shutil.copy2(mo, lc_messages)
            for lib_domain in LIBRARY_DOMAINS:
                lib_mo = self.prefix / "share" / "locale" / lang / "LC_MESSAGES" / f"{lib_domain}.mo"
                if lib_mo.exists():
                    shutil.copy2(lib_mo, lc_messages)

    def check(self):
        bad = []
        for path in macho_files(self.contents):
            libs, rpaths = load_commands(path)
            for ref in libs + rpaths:
                if not (ref.startswith("@") or is_system(ref)):
                    bad.append(f"{path.relative_to(self.app)}: {ref}")
        if bad:
            fail("bundle still refers to files outside of it:\n  " + "\n  ".join(bad))

    def sign(self):
        # Sign the nested code first, then the bundle (sealing the resources)
        for path in macho_files(self.contents):
            if path.parent.name != "MacOS":
                codesign(path)
        codesign(self.app)
        run("codesign", "--verify", "--deep", "--strict", self.app)


def make_dmg(app, dmg):
    dmg = Path(dmg)
    with tempfile.TemporaryDirectory() as tmp:
        shutil.copytree(app, Path(tmp) / app.name, symlinks=True)
        (Path(tmp) / "Applications").symlink_to("/Applications")
        dmg.unlink(missing_ok=True)
        run("hdiutil", "create", "-quiet", "-volname", app.stem, "-srcfolder", tmp,
            "-format", "UDZO", "-ov", dmg)
    print(f"Created {dmg}")


def default_deps_prefix():
    prefix = os.environ.get("HOMEBREW_PREFIX")
    if not prefix and shutil.which("brew"):
        prefix = output("brew", "--prefix").strip()
    return prefix or "/opt/homebrew"


def make_standalone(args):
    src = Path(args.app)
    app = Path(args.output)
    if app.exists():
        shutil.rmtree(app)
    app.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(src, app, symlinks=True)

    bundler = Bundler(app, args.deps_prefix)
    domain = next(iter(macho_files(app / "Contents" / "MacOS"))).name
    bundler.bundle_libraries()
    bundler.bundle_pixbuf_loaders()
    bundler.bundle_gio_modules()
    bundler.bundle_schemas()
    bundler.bundle_icons()
    if args.po_dir:
        bundler.bundle_locales(args.po_dir, domain)
    bundler.check()
    bundler.sign()
    print(f"Created {app}")

    if args.dmg:
        make_dmg(app, args.dmg)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)

    p = sub.add_parser("icns")
    p.add_argument("rsvg_convert")
    p.add_argument("iconutil")
    p.add_argument("svg")
    p.add_argument("output")
    p.set_defaults(func=make_icns)

    p = sub.add_parser("app")
    p.add_argument("executable")
    p.add_argument("info_plist")
    p.add_argument("icns")
    p.add_argument("svg")
    p.add_argument("output")
    p.set_defaults(func=make_app)

    p = sub.add_parser("standalone")
    p.add_argument("--po-dir", help="build directory of the translations")
    p.add_argument("--deps-prefix", default=default_deps_prefix(),
                   help="prefix the dependencies are installed in (default: Homebrew's)")
    p.add_argument("--dmg", help="also create this disk image")
    p.add_argument("app", help="development bundle")
    p.add_argument("output", help="bundle to create")
    p.set_defaults(func=make_standalone)

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
