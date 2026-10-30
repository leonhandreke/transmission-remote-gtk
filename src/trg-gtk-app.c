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

#include <glib/gi18n.h>
#include <gtk/gtk.h>

#ifdef GDK_WINDOWING_MACOS
#include <CoreFoundation/CoreFoundation.h>
#endif

#include "trg-gtk-app.h"
#include "trg-main-window.h"
#include "trg-menu-bar.h"
#include "util.h"

enum {
    PROP_0,
    PROP_CLIENT,
    N_PROPS
};

struct _TrgGtkApp {
    GtkApplication parent;

    TrgClient *client;
};

G_DEFINE_TYPE(TrgGtkApp, trg_gtk_app, GTK_TYPE_APPLICATION)

static void trg_gtk_app_get_property(GObject *object, guint property_id, GValue *value,
                                     GParamSpec *pspec)
{
    TrgGtkApp *self = TRG_GTK_APP(object);
    switch (property_id) {
    case PROP_CLIENT:
        g_value_set_pointer(value, self->client);
        break;
    default:
        G_OBJECT_WARN_INVALID_PROPERTY_ID(object, property_id, pspec);
        break;
    }
}

static void trg_gtk_app_set_property(GObject *object, guint property_id, const GValue *value,
                                     GParamSpec *pspec)
{
    TrgGtkApp *self = TRG_GTK_APP(object);
    switch (property_id) {
    case PROP_CLIENT:
        self->client = g_value_get_pointer(value);
        break;
    default:
        G_OBJECT_WARN_INVALID_PROPERTY_ID(object, property_id, pspec);
        break;
    }
}

/* "app.quit" (File -> Quit, Ctrl+Q) closes the window, rather than quitting
 * straight away, so that it can save its state. */
static void trg_gtk_app_quit_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    GList *windows = gtk_application_get_windows(GTK_APPLICATION(data));

    if (windows)
        gtk_window_close(GTK_WINDOW(windows->data));
    else
        g_application_quit(G_APPLICATION(data));
}

static TrgMainWindow *trg_gtk_app_get_main_window(GtkApplication *app)
{
    GList *li;

    for (li = gtk_application_get_windows(app); li; li = g_list_next(li))
        if (TRG_IS_MAIN_WINDOW(li->data))
            return TRG_MAIN_WINDOW(li->data);

    return NULL;
}

#ifdef GDK_WINDOWING_MACOS
/* macOS: About and Preferences in the application menu. */
static void trg_gtk_app_about_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *window = trg_gtk_app_get_main_window(GTK_APPLICATION(data));

    if (window)
        gtk_widget_activate_action(GTK_WIDGET(window), "win.about", NULL);
}

static void trg_gtk_app_preferences_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *window = trg_gtk_app_get_main_window(GTK_APPLICATION(data));

    if (window)
        gtk_widget_activate_action(GTK_WIDGET(window), "win.local-prefs", NULL);
}

/* macOS: Close Window, Minimize and Zoom act on the focused window, which
 * can be a dialog. (The "window." actions in the menu bar would always act on
 * the main window, the only one that belongs to the application.) */
static GtkWindow *trg_gtk_app_get_focused_window(GtkApplication *app)
{
    GListModel *toplevels = gtk_window_get_toplevels();
    GtkWindow *focused = NULL;
    guint i;

    for (i = 0; !focused && i < g_list_model_get_n_items(toplevels); i++) {
        GtkWindow *window = g_list_model_get_item(toplevels, i);

        if (gtk_window_is_active(window))
            focused = window;
        g_object_unref(window);
    }

    return focused ? focused : GTK_WINDOW(trg_gtk_app_get_main_window(app));
}

static void trg_gtk_app_window_action_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    const gchar *name = g_action_get_name(G_ACTION(action));
    GtkWindow *window = trg_gtk_app_get_focused_window(GTK_APPLICATION(data));

    if (!window)
        return;

    if (!g_strcmp0(name, "close-window"))
        gtk_window_close(window);
    else if (!g_strcmp0(name, "minimize"))
        gtk_window_minimize(window);
    else if (gtk_window_is_maximized(window))
        gtk_window_unmaximize(window);
    else
        gtk_window_maximize(window);
}

/* GTK's macOS backend doesn't pass on the system's light/dark appearance, so
 * follow it here: AppleInterfaceStyle is "Dark" in dark mode, and changes are
 * announced with a distributed notification. */
static void trg_gtk_app_macos_update_color_scheme(void)
{
    CFPropertyListRef style;
    gboolean dark;

    CFPreferencesAppSynchronize(kCFPreferencesAnyApplication);
    style = CFPreferencesCopyAppValue(CFSTR("AppleInterfaceStyle"), kCFPreferencesAnyApplication);
    dark = style && CFGetTypeID(style) == CFStringGetTypeID()
        && CFStringCompare(style, CFSTR("Dark"), 0) == kCFCompareEqualTo;
    if (style)
        CFRelease(style);

#if GTK_CHECK_VERSION(4, 20, 0)
    g_object_set(gtk_settings_get_default(), "gtk-interface-color-scheme",
                 dark ? GTK_INTERFACE_COLOR_SCHEME_DARK : GTK_INTERFACE_COLOR_SCHEME_LIGHT, NULL);
#else
    g_object_set(gtk_settings_get_default(), "gtk-application-prefer-dark-theme", dark, NULL);
#endif
}

static void trg_gtk_app_macos_appearance_changed(CFNotificationCenterRef center, void *observer,
                                                 CFNotificationName name, const void *object,
                                                 CFDictionaryRef user_info)
{
    trg_gtk_app_macos_update_color_scheme();
}

/* macOS conventions: the menu in the system menu bar, with About and
 * Preferences in the application menu, window actions, and following dark
 * mode. */
static void trg_gtk_app_macos_startup(GtkApplication *app, TrgMainWindow *window)
{
    static const GActionEntry macos_entries[] = {
        { "about", trg_gtk_app_about_cb, NULL, NULL, NULL, { 0 } },
        { "preferences", trg_gtk_app_preferences_cb, NULL, NULL, NULL, { 0 } },
        { "close-window", trg_gtk_app_window_action_cb, NULL, NULL, NULL, { 0 } },
        { "minimize", trg_gtk_app_window_action_cb, NULL, NULL, NULL, { 0 } },
        { "zoom", trg_gtk_app_window_action_cb, NULL, NULL, NULL, { 0 } },
    };

    g_action_map_add_action_entries(G_ACTION_MAP(app), macos_entries, G_N_ELEMENTS(macos_entries),
                                    app);
    gtk_application_set_menubar(app, trg_main_window_get_menu_model(window));

    trg_gtk_app_macos_update_color_scheme();
    CFNotificationCenterAddObserver(CFNotificationCenterGetDistributedCenter(), NULL,
                                    trg_gtk_app_macos_appearance_changed,
                                    CFSTR("AppleInterfaceThemeChangedNotification"), NULL,
                                    CFNotificationSuspensionBehaviorDeliverImmediately);
}
#endif

static void trg_gtk_app_startup(GApplication *app, gpointer userdata)
{
    static const GActionEntry app_entries[] = {
        { "quit", trg_gtk_app_quit_cb, NULL, NULL, NULL, { 0 } },
    };
    TrgGtkApp *self = TRG_GTK_APP(app);
    TrgMainWindow *window;

    g_action_map_add_action_entries(G_ACTION_MAP(app), app_entries, G_N_ELEMENTS(app_entries), app);

    window = trg_main_window_new(self->client);
    gtk_window_set_application(GTK_WINDOW(window), GTK_APPLICATION(app));
    trg_menu_bar_set_accels(GTK_APPLICATION(app));
#ifdef GDK_WINDOWING_MACOS
    if (trg_is_macos())
        trg_gtk_app_macos_startup(GTK_APPLICATION(app), window);
#endif
    gtk_window_present(GTK_WINDOW(window));
}

/* Files opened from the Finder or dropped on the Dock icon on macOS, whether
 * or not the application was already running. */
static void trg_gtk_app_open(GApplication *application, GFile **files, gint n_files,
                             const gchar *hint)
{
    TrgMainWindow *window = trg_gtk_app_get_main_window(GTK_APPLICATION(application));
    gchar **uris;
    gint i, j;

    if (!window)
        return;

    uris = g_new0(gchar *, n_files + 1);
    for (i = 0, j = 0; i < n_files; i++) {
        gchar *path = g_file_get_path(files[i]);
        if (path)
            uris[j++] = path;
    }

    trg_main_window_open(window, uris);
}

static int trg_gtk_app_command_line(GApplication *application, GApplicationCommandLine *cmdline)
{
    GList *windows = gtk_application_get_windows(GTK_APPLICATION(application));
    TrgMainWindow *window;
    gchar **argv;

    if (!windows || !windows->data)
        return 1;

    window = TRG_MAIN_WINDOW(windows->data);
    argv = g_application_command_line_get_arguments(cmdline, NULL);

    if (g_application_command_line_get_is_remote(cmdline)) {
        if (!argv[0]) {
            gtk_window_present(GTK_WINDOW(window));
            g_strfreev(argv);
        } else {
            return trg_add_from_filename(window, argv);
        }
    } else {
        trg_main_window_set_start_args(window, argv);
        auto_connect_if_required(TRG_MAIN_WINDOW(windows->data));
    }

    return 0;
}

static void shift_args(gchar **argv, int i)
{
    gint j;
    g_free(argv[i]);
    for (j = i; argv[j]; j++)
        argv[j] = argv[j + 1];
}

static gboolean test_local_cmdline(GApplication *application, gchar ***arguments, gint *exit_status)
{
    gchar **argv;
    gchar *cwd = g_get_current_dir();
    gchar *tmp;
    gint i;

    argv = *arguments;
    shift_args(argv, 0);

    i = 0;
    while (argv[i]) {
        if (!is_url(argv[i]) && !is_magnet(argv[i]) && g_file_test(argv[i], G_FILE_TEST_IS_REGULAR)
            && !g_path_is_absolute(argv[i])) {
            tmp = g_build_path(G_DIR_SEPARATOR_S, cwd, argv[i], NULL);
            g_free(argv[i]);
            argv[i] = tmp;
        }
        i++;
    }

    *exit_status = 0;

    g_free(cwd);

    return FALSE;
}

static void trg_gtk_app_class_init(TrgGtkAppClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);
    GApplicationClass *app_class = G_APPLICATION_CLASS(klass);

    object_class->get_property = trg_gtk_app_get_property;
    object_class->set_property = trg_gtk_app_set_property;
    app_class->local_command_line = test_local_cmdline;
    // app_class->startup = trg_gtk_app_startup;
    app_class->command_line = trg_gtk_app_command_line;
    app_class->open = trg_gtk_app_open;

    g_object_class_install_property(
        object_class, PROP_CLIENT,
        g_param_spec_pointer("trg-client", "TClient", _("Client"),
                             G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_STRINGS));
}

static void trg_gtk_app_init(TrgGtkApp *self)
{
    g_application_set_inactivity_timeout(G_APPLICATION(self), 10000);
    g_signal_connect(self, "command-line", G_CALLBACK(trg_gtk_app_command_line), NULL);
    g_signal_connect(self, "startup", G_CALLBACK(trg_gtk_app_startup), NULL);
}

TrgGtkApp *trg_gtk_app_new(TrgClient *client)
{
    GApplicationFlags flags = G_APPLICATION_HANDLES_COMMAND_LINE;

#ifdef GDK_WINDOWING_MACOS
    /* The macOS backend passes on files opened from the Finder as "open" */
    flags |= G_APPLICATION_HANDLES_OPEN;
#endif

    return g_object_new(TRG_TYPE_GTK_APP, "application-id", APPLICATION_ID, "flags", flags,
                        "trg-client", client, NULL);
}
