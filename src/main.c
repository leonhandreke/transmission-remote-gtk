/*
 * transmission-remote-gtk - A GTK RPC client to Transmission
 * Copyright (C) 2011-2013  Alan Fitton

 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 2 of the License, or
 * (at your option) any later version.

 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.

 * You should have received a copy of the GNU General Public License along
 * with this program; if not, write to the Free Software Foundation, Inc.,
 * 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.
 */

#include "config.h"

#include <glib-object.h>
#include <glib/gi18n.h>
#include <gtk/gtk.h>

#ifdef __APPLE__
#include <limits.h>
#include <mach-o/dyld.h>
#endif

#include "trg-client.h"
#include "trg-gtk-app.h"
#include "trg-main-window.h"

/* Handle arguments and start the main window. */

#ifdef __APPLE__
/* The Resources directory of the application bundle we're run from, or NULL */
static gchar *macos_get_bundle_resources_dir(void)
{
    char path[PATH_MAX];
    uint32_t size = sizeof(path);

    if (_NSGetExecutablePath(path, &size) != 0)
        return NULL;

    g_autofree gchar *exe_dir = g_path_get_dirname(path);
    if (!g_str_has_suffix(exe_dir, "/Contents/MacOS"))
        return NULL;

    return g_canonicalize_filename("../Resources", exe_dir);
}

static void macos_prepend_data_dir(const gchar *dir)
{
    const gchar *data_dirs = g_getenv("XDG_DATA_DIRS");
    if (!data_dirs || !*data_dirs)
        data_dirs = "/usr/local/share:/usr/share";

    g_auto(GStrv) dirs = g_strsplit(data_dirs, G_SEARCHPATH_SEPARATOR_S, -1);
    if (g_strv_contains((const gchar *const *)dirs, dir))
        return;

    g_autofree gchar *value = g_strjoin(G_SEARCHPATH_SEPARATOR_S, dir, data_dirs, NULL);
    g_setenv("XDG_DATA_DIRS", value, TRUE);
}

/*
 * Make GTK find its data (icons, GSettings schemas, ...). This has to happen
 * before GLib reads the environment.
 *
 * A self-contained bundle (made by macos/bundle.py) carries its own libraries
 * and data, in Contents/Frameworks and Contents/Resources/{lib,share}.
 * Otherwise, use the data in our prefix (usually Homebrew's), which isn't in
 * the default XDG_DATA_DIRS, plus the bundle's own data if there is any.
 */
static void macos_setup_environment(void)
{
    g_autofree gchar *resources = macos_get_bundle_resources_dir();
    g_autofree gchar *libdir = resources ? g_build_filename(resources, "lib", NULL) : NULL;
    g_autofree gchar *datadir = resources ? g_build_filename(resources, "share", NULL) : NULL;

    if (libdir && g_file_test(libdir, G_FILE_TEST_IS_DIR)) {
        g_autofree gchar *pixbuf_cache
            = g_build_filename(libdir, "gdk-pixbuf-2.0", "2.10.0", "loaders.cache", NULL);
        g_autofree gchar *gio_modules = g_build_filename(libdir, "gio", "modules", NULL);

        g_setenv("XDG_DATA_DIRS", datadir, TRUE);
        g_setenv("GDK_PIXBUF_MODULE_FILE", pixbuf_cache, TRUE);
        g_setenv("GIO_MODULE_DIR", gio_modules, TRUE);
        g_setenv("GTK_DATA_PREFIX", resources, TRUE);
        g_setenv("GTK_EXE_PREFIX", resources, TRUE);
        return;
    }

    macos_prepend_data_dir(DATADIR);
    if (datadir && g_file_test(datadir, G_FILE_TEST_IS_DIR))
        macos_prepend_data_dir(datadir);
}
#endif

/* Get platform dependendent localedir */
static void bindtext_wrapper(void)
{
    bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8");
    textdomain(GETTEXT_PACKAGE);
#ifdef G_OS_WIN32
    gchar *moddir = g_win32_get_package_installation_directory_of_module(NULL);

    gchar *localedir = g_build_path(G_DIR_SEPARATOR_S, moddir, "share", "locale", NULL);

    bindtextdomain(GETTEXT_PACKAGE, localedir);
    g_free(moddir);
    g_free(localedir);
#elif defined(__APPLE__)
    g_autofree gchar *resources = macos_get_bundle_resources_dir();
    g_autofree gchar *localedir
        = resources ? g_build_filename(resources, "share", "locale", NULL) : NULL;

    if (localedir && g_file_test(localedir, G_FILE_TEST_IS_DIR)) {
        bindtextdomain(GETTEXT_PACKAGE, localedir);
        /* GTK binds its translations to its build-time prefix on
         * initialisation, use the bundle's copies instead. */
        if (gtk_init_check())
            bindtextdomain("gtk40", localedir);
    } else {
        bindtextdomain(GETTEXT_PACKAGE, LOCALEDIR);
    }
#else
    bindtextdomain(GETTEXT_PACKAGE, LOCALEDIR);
#endif
}

int main(int argc, char *argv[])
{
#ifdef __APPLE__
    macos_setup_environment();
#endif
    g_set_application_name(PACKAGE_NAME);
    g_set_prgname(APPLICATION_ID);
    bindtext_wrapper();

    g_autoptr(TrgClient) client = trg_client_new();
    g_autoptr(TrgGtkApp) gtk_app = trg_gtk_app_new(client);
    return g_application_run(G_APPLICATION(gtk_app), argc, argv);
}
