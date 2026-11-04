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

#include "trg-main-window.h"
#include "trg-menu-bar.h"
#include "trg-prefs.h"
#include "trg-torrent-tree-view.h"
#include "trg-tree-view.h"
#include "util.h"

/* The main menu, as a GMenuModel. The menu items activate actions installed
 * on the main window ("win." prefix, see trg-main-window.c), apart from the
 * stateful view actions (check and radio items) which are created here as
 * they're backed by preferences. This is shown in the window with a
 * GtkPopoverMenuBar, or on macOS in the system menu bar with
 * gtk_application_set_menubar(). There GTK provides the application menu,
 * with About, Preferences and Quit (app.about, app.preferences and app.quit),
 * so those aren't repeated in the other menus.
 */

enum {
    PROP_0,
    PROP_PREFS,
    PROP_MAIN_WINDOW,
    PROP_TORRENT_TREE_VIEW
};

#define G_DATAKEY_CONF_KEY "conf-key"

struct _TrgMenuBar {
    GObject parent;

    GMenu *menu;
    GMenu *queue_section;
    GMenuModel *queue_items;
    gboolean queues_shown;
    TrgPrefs *prefs;
    TrgMainWindow *main_window;
    TrgTorrentTreeView *torrent_tree_view;
};

G_DEFINE_TYPE(TrgMenuBar, trg_menu_bar, G_TYPE_OBJECT)

static const gchar *connected_actions[]
    = { "add", "add-url", "disconnect", "remote-prefs", "view-stats", "resume-all", "pause-all" };

static const gchar *torrent_actions[]
    = { "properties", "copy-magnet", "remove",     "delete",      "resume",
        "pause",      "verify",      "reannounce", "move",        "start-now",
        "up-queue",   "down-queue",  "top-queue",  "bottom-queue" };

static const gchar *view_dependent_actions[]
    = { "filter-dirs", "filter-trackers", "directories-first" };

static void trg_menu_bar_set_enabled(TrgMenuBar *mb, const gchar **names, guint n, gboolean enabled)
{
    guint i;

    for (i = 0; i < n; i++) {
        GAction *action = g_action_map_lookup_action(G_ACTION_MAP(mb->main_window), names[i]);
        if (action)
            g_simple_action_set_enabled(G_SIMPLE_ACTION(action), enabled);
    }
}

/* Appends (copies of) the items of a menu from the GtkBuilder file */
static void trg_menu_append_items(GMenu *menu, GMenuModel *items)
{
    gint i;

    for (i = 0; i < g_menu_model_get_n_items(items); i++) {
        GMenuItem *item = g_menu_item_new_from_model(items, i);
        g_menu_append_item(menu, item);
        g_object_unref(item);
    }
}

void trg_menu_bar_set_supports_queues(TrgMenuBar *mb, gboolean supportsQueues)
{
    /* Menu models have no hidden items, so remove/re-add the queue items. An
     * empty section isn't shown, along with its separator. */
    if (supportsQueues == mb->queues_shown)
        return;

    if (supportsQueues)
        trg_menu_append_items(mb->queue_section, mb->queue_items);
    else
        g_menu_remove_all(mb->queue_section);

    mb->queues_shown = supportsQueues;
}

void trg_menu_bar_connected_change(TrgMenuBar *mb, gboolean connected)
{
    trg_menu_bar_set_enabled(mb, connected_actions, G_N_ELEMENTS(connected_actions), connected);
}

void trg_menu_bar_torrent_actions_sensitive(TrgMenuBar *mb, gboolean sensitive)
{
    trg_menu_bar_set_enabled(mb, torrent_actions, G_N_ELEMENTS(torrent_actions), sensitive);
}

static void trg_menu_bar_set_property(GObject *object, guint prop_id, const GValue *value,
                                      GParamSpec *pspec G_GNUC_UNUSED)
{
    TrgMenuBar *self = TRG_MENU_BAR(object);
    switch (prop_id) {
    case PROP_PREFS:
        self->prefs = g_value_get_object(value);
        break;
    case PROP_MAIN_WINDOW:
        self->main_window = g_value_get_object(value);
        break;
    case PROP_TORRENT_TREE_VIEW:
        self->torrent_tree_view = g_value_get_object(value);
        break;
    }
}

static void trg_menu_bar_get_property(GObject *object, guint property_id, GValue *value,
                                      GParamSpec *pspec)
{
    G_OBJECT_WARN_INVALID_PROPERTY_ID(object, property_id, pspec);
}

/* "win.style" has the nick of the style as its state, so it can be the
 * target of the items in the GtkBuilder file */
static const gchar *style_nicks[] = {
    [TRG_STYLE_TR] = "transmission",
    [TRG_STYLE_CLASSIC] = "classic",
    [TRG_STYLE_TR_COMPACT] = "transmission-compact",
};

static GVariant *trg_menu_bar_style_state(TrgPrefs *p)
{
    gint64 style = trg_prefs_get_int(p, TRG_PREFS_KEY_STYLE, TRG_PREFS_GLOBAL);

    if (style < 0 || style >= (gint64)G_N_ELEMENTS(style_nicks))
        style = TRG_STYLE_TR;

    return g_variant_new_string(style_nicks[style]);
}

static void style_change_state(GSimpleAction *action, GVariant *value, gpointer data)
{
    const gchar *nick = g_variant_get_string(value, NULL);
    guint i;

    for (i = 0; i < G_N_ELEMENTS(style_nicks); i++) {
        if (g_strcmp0(nick, style_nicks[i]) == 0) {
            g_simple_action_set_state(action, value);
            trg_prefs_set_int(TRG_PREFS(data), TRG_PREFS_KEY_STYLE, i, TRG_PREFS_GLOBAL);
            break;
        }
    }
}

static void style_update(TrgPrefs *p, const gchar *updatedKey, gpointer data)
{
    GSimpleAction *action = G_SIMPLE_ACTION(data);
    g_autoptr(GVariant) state = g_action_get_state(G_ACTION(action));
    g_autoptr(GVariant) value = g_variant_ref_sink(trg_menu_bar_style_state(p));

    if (g_strcmp0(updatedKey, TRG_PREFS_KEY_STYLE) == 0 && !g_variant_equal(state, value))
        g_simple_action_set_state(action, value);
}

static void trg_menu_bar_style_action_new(TrgMenuBar *mb)
{
    GSimpleAction *action = g_simple_action_new_stateful("style", G_VARIANT_TYPE_STRING,
                                                         trg_menu_bar_style_state(mb->prefs));

    g_signal_connect(action, "change-state", G_CALLBACK(style_change_state), mb->prefs);
    g_signal_connect_object(mb->prefs, "pref-changed", G_CALLBACK(style_update), action, 0);
    g_action_map_add_action(G_ACTION_MAP(mb->main_window), G_ACTION(action));
    g_object_unref(action);
}

static void view_menu_item_change_state(GSimpleAction *action, GVariant *value, gpointer data)
{
    TrgPrefs *p = TRG_PREFS(data);
    const gchar *key = (gchar *)g_object_get_data(G_OBJECT(action), G_DATAKEY_CONF_KEY);

    g_simple_action_set_state(action, value);
    trg_prefs_set_bool(p, key, g_variant_get_boolean(value), TRG_PREFS_GLOBAL);
}

static void view_menu_bar_toggled_dependency_cb(GObject *dependency, GParamSpec *pspec,
                                                gpointer data)
{
    TrgMenuBar *mb = TRG_MENU_BAR(data);
    g_autoptr(GVariant) state = g_action_get_state(G_ACTION(dependency));

    trg_menu_bar_set_enabled(mb, view_dependent_actions, G_N_ELEMENTS(view_dependent_actions),
                             g_variant_get_boolean(state));
}

/* Preferences can also be changed from the local preferences dialog */
static void trg_menu_bar_view_item_update(TrgPrefs *p, const gchar *updatedKey, gpointer data)
{
    GSimpleAction *action = G_SIMPLE_ACTION(data);
    const gchar *key = (gchar *)g_object_get_data(G_OBJECT(action), G_DATAKEY_CONF_KEY);
    g_autoptr(GVariant) state = g_action_get_state(G_ACTION(action));
    gboolean value;

    if (g_strcmp0(updatedKey, key))
        return;

    value = trg_prefs_get_bool(p, key, TRG_PREFS_GLOBAL);
    if (value != g_variant_get_boolean(state))
        g_simple_action_set_state(action, g_variant_new_boolean(value));
}

static GSimpleAction *trg_menu_bar_view_action_new(TrgMenuBar *mb, const gchar *name,
                                                   GVariant *state, const gchar *key)
{
    GSimpleAction *action = g_simple_action_new_stateful(name, NULL, state);

    g_object_set_data_full(G_OBJECT(action), G_DATAKEY_CONF_KEY, g_strdup(key), g_free);
    g_action_map_add_action(G_ACTION_MAP(mb->main_window), G_ACTION(action));
    g_object_unref(action);

    return action;
}

/* A boolean preference, as stateful "win.<name>" action */
static void trg_menu_bar_view_item_new(TrgMenuBar *mb, const gchar *name, const gchar *key,
                                       const gchar *dependency)
{
    GSimpleAction *action = trg_menu_bar_view_action_new(
        mb, name, g_variant_new_boolean(trg_prefs_get_bool(mb->prefs, key, TRG_PREFS_GLOBAL)), key);

    if (dependency) {
        GAction *dep = g_action_map_lookup_action(G_ACTION_MAP(mb->main_window), dependency);
        g_autoptr(GVariant) state = g_action_get_state(dep);
        g_simple_action_set_enabled(action, g_variant_get_boolean(state));
    }

    g_signal_connect(action, "change-state", G_CALLBACK(view_menu_item_change_state), mb->prefs);
    g_signal_connect_object(mb->prefs, "pref-changed", G_CALLBACK(trg_menu_bar_view_item_update),
                            action, 0);
}

static void trg_menu_bar_view_menu_fill(TrgMenuBar *mb, GMenu *viewSection)
{
    GMenu *sortMenu;
    GAction *viewStates;

    trg_menu_bar_style_action_new(mb);

    sortMenu = trg_tree_view_sort_menu(TRG_TREE_VIEW(mb->torrent_tree_view),
                                       G_ACTION_MAP(mb->main_window));
    g_menu_insert_submenu(viewSection, 0, _("Sort"), G_MENU_MODEL(sortMenu));
    g_object_unref(sortMenu);

    trg_menu_bar_view_item_new(mb, "view-states", TRG_PREFS_KEY_SHOW_STATE_SELECTOR, NULL);
    trg_menu_bar_view_item_new(mb, "filter-dirs", TRG_PREFS_KEY_FILTER_DIRS, "view-states");
    trg_menu_bar_view_item_new(mb, "filter-trackers", TRG_PREFS_KEY_FILTER_TRACKERS, "view-states");
    trg_menu_bar_view_item_new(mb, "directories-first", TRG_PREFS_KEY_DIRECTORIES_FIRST,
                               "view-states");

    viewStates = g_action_map_lookup_action(G_ACTION_MAP(mb->main_window), "view-states");
    g_signal_connect_object(viewStates, "notify::state",
                            G_CALLBACK(view_menu_bar_toggled_dependency_cb), mb, 0);

    trg_menu_bar_view_item_new(mb, "view-notebook", TRG_PREFS_KEY_SHOW_NOTEBOOK, NULL);
}

static void trg_menu_bar_file_connect_menu_fill(GMenu *menu, TrgPrefs *p)
{
    GList *profiles = json_array_get_elements(trg_prefs_get_profiles(p));
    GList *li;
    gint i;

    for (li = profiles, i = 0; li; li = g_list_next(li), i++) {
        JsonObject *profile = json_node_get_object((JsonNode *)li->data);
        const gchar *name_value;
        GMenuItem *item;

        if (json_object_has_member(profile, TRG_PREFS_KEY_PROFILE_NAME)) {
            name_value = json_object_get_string_member(profile, TRG_PREFS_KEY_PROFILE_NAME);
        } else {
            name_value = _(TRG_PROFILE_NAME_DEFAULT);
        }

        /* The connect action's state is the current profile, so the items
         * display as radio items like they did in GTK 3. */
        item = g_menu_item_new(name_value, NULL);
        g_menu_item_set_action_and_target(item, "win.connect", "i", i);
        g_menu_append_item(menu, item);
        g_object_unref(item);
    }

    g_list_free(profiles);
}

static void trg_menu_bar_file_connect_menu_refresh(TrgPrefs *p, gpointer data)
{
    GMenu *menu = G_MENU(data);

    g_menu_remove_all(menu);
    trg_menu_bar_file_connect_menu_fill(menu, p);
}

/* Returns a menu of the connection profiles, which keeps itself up to date. */
GMenu *trg_menu_bar_file_connect_menu_new(TrgMainWindow *win, TrgPrefs *p)
{
    GMenu *menu = g_menu_new();

    trg_menu_bar_file_connect_menu_fill(menu, p);
    g_signal_connect_object(p, "pref-profile-changed",
                            G_CALLBACK(trg_menu_bar_file_connect_menu_refresh), menu, 0);

    return menu;
}

static gint trg_menu_bar_current_profile_index(TrgPrefs *p)
{
    GList *profiles = json_array_get_elements(trg_prefs_get_profiles(p));
    JsonObject *currentProfile = trg_prefs_get_profile(p);
    GList *li;
    gint i, index = -1;

    for (li = profiles, i = 0; li; li = g_list_next(li), i++) {
        if (json_node_get_object((JsonNode *)li->data) == currentProfile) {
            index = i;
            break;
        }
    }

    g_list_free(profiles);

    return index;
}

static void menu_bar_refresh_menu(TrgPrefs *p, gpointer data)
{
    TrgMenuBar *mb = TRG_MENU_BAR(data);
    GAction *connect = g_action_map_lookup_action(G_ACTION_MAP(mb->main_window), "connect");

    if (connect)
        g_simple_action_set_state(G_SIMPLE_ACTION(connect),
                                  g_variant_new_int32(trg_menu_bar_current_profile_index(p)));
}

static void trg_menu_bar_constructed(GObject *object)
{
    TrgMenuBar *mb = TRG_MENU_BAR(object);
    gboolean macos = trg_is_macos();
    GSimpleAction *connect;
    GtkBuilder *builder;
    GMenu *connectMenu, *fileMenu;

    G_OBJECT_CLASS(trg_menu_bar_parent_class)->constructed(object);

    /* "win.connect" connects to the profile index given (-1 for the current
     * one) and its state is the current profile. */
    connect = G_SIMPLE_ACTION(g_action_map_lookup_action(G_ACTION_MAP(mb->main_window), "connect"));
    if (connect)
        g_simple_action_set_state(
            connect, g_variant_new_int32(trg_menu_bar_current_profile_index(mb->prefs)));

    builder = gtk_builder_new_from_resource("/io/github/TransmissionRemoteGtk/ui/trg-menu-bar.ui");
    mb->menu = g_object_ref(G_MENU(gtk_builder_get_object(builder, "main_menu")));

    /* The connect submenu of the File menu */
    connectMenu = G_MENU(gtk_builder_get_object(builder, "connect_menu"));
    trg_menu_bar_file_connect_menu_fill(connectMenu, mb->prefs);
    g_signal_connect_object(mb->prefs, "pref-profile-changed",
                            G_CALLBACK(trg_menu_bar_file_connect_menu_refresh), connectMenu, 0);

    /* Quit, or on macOS (where it's in the application menu) Close Window */
    fileMenu = G_MENU(gtk_builder_get_object(builder, "file_menu"));
    g_menu_remove(fileMenu, g_menu_model_get_n_items(G_MENU_MODEL(fileMenu)) - (macos ? 2 : 1));

    mb->queue_section = g_object_ref(G_MENU(gtk_builder_get_object(builder, "queue_section")));
    mb->queue_items = g_object_ref(G_MENU_MODEL(gtk_builder_get_object(builder, "queue_items")));
    trg_menu_append_items(mb->queue_section, mb->queue_items);
    mb->queues_shown = TRUE;

    /* macOS has Preferences in the application menu */
    if (macos)
        g_menu_remove(G_MENU(gtk_builder_get_object(builder, "options_menu")), 0);

    trg_menu_bar_view_menu_fill(mb, G_MENU(gtk_builder_get_object(builder, "view_section")));

    if (macos) {
        GMenuItem *edit = g_menu_item_new_from_model(
            G_MENU_MODEL(gtk_builder_get_object(builder, "macos_edit_menu")), 0);
        g_menu_insert_item(mb->menu, 1, edit);
        g_object_unref(edit);

        trg_menu_append_items(mb->menu,
                              G_MENU_MODEL(gtk_builder_get_object(builder, "macos_window_menu")));
    } else {
        trg_menu_append_items(mb->menu, G_MENU_MODEL(gtk_builder_get_object(builder, "help_menu")));
    }

    g_object_unref(builder);

    trg_menu_bar_connected_change(mb, FALSE);
    trg_menu_bar_torrent_actions_sensitive(mb, FALSE);

    g_signal_connect_object(mb->prefs, "pref-profile-changed", G_CALLBACK(menu_bar_refresh_menu),
                            object, 0);
}

static void trg_menu_bar_dispose(GObject *object)
{
    TrgMenuBar *mb = TRG_MENU_BAR(object);

    g_clear_object(&mb->queue_section);
    g_clear_object(&mb->queue_items);
    g_clear_object(&mb->menu);

    G_OBJECT_CLASS(trg_menu_bar_parent_class)->dispose(object);
}

static void trg_menu_bar_class_init(TrgMenuBarClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);
    object_class->get_property = trg_menu_bar_get_property;
    object_class->set_property = trg_menu_bar_set_property;
    object_class->constructed = trg_menu_bar_constructed;
    object_class->dispose = trg_menu_bar_dispose;

    g_object_class_install_property(
        object_class, PROP_PREFS,
        g_param_spec_object("prefs", "prefs", "Prefs", TRG_TYPE_PREFS,
                            G_PARAM_WRITABLE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    g_object_class_install_property(
        object_class, PROP_MAIN_WINDOW,
        g_param_spec_object("mainwin", "mainwin", "mainwin", TRG_TYPE_MAIN_WINDOW,
                            G_PARAM_WRITABLE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    g_object_class_install_property(
        object_class, PROP_TORRENT_TREE_VIEW,
        g_param_spec_object("torrent-tree-view", "torrent-tree-view", "torrent-tree-view",
                            TRG_TYPE_TORRENT_TREE_VIEW,
                            G_PARAM_WRITABLE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));
}

static void trg_menu_bar_init(TrgMenuBar *self)
{
}

GMenuModel *trg_menu_bar_get_model(TrgMenuBar *mb)
{
    return G_MENU_MODEL(mb->menu);
}

/* On macOS, <Primary> means Command, which GTK itself doesn't do (it's
 * <Meta> there). Returns a newly allocated list. */
static gchar **trg_menu_bar_platform_accels(const gchar *const *accels, gboolean macos)
{
    gchar **result = g_new0(gchar *, g_strv_length((gchar **)accels) + 1);
    guint i;

    for (i = 0; accels[i]; i++) {
        if (macos) {
            g_auto(GStrv) parts = g_strsplit(accels[i], "<Primary>", -1);
            result[i] = g_strjoinv("<Meta>", parts);
        } else {
            result[i] = g_strdup(accels[i]);
        }
    }

    return result;
}

/* Keyboard shortcuts for the menu actions. These are application wide, which
 * is also what makes the menus display them. */
void trg_menu_bar_set_accels(GtkApplication *app)
{
    typedef struct {
        const gchar *action;
        const gchar *accels[3];
    } trg_accel;
    static const trg_accel accels[] = {
        { "win.disconnect", { "<Primary>d", NULL } },
        { "win.add", { "<Primary>o", NULL } },
        { "win.add-url", { "<Primary>u", NULL } },
        { "app.quit", { "<Primary>q", NULL } },
        { "win.properties", { "<Primary>i", NULL } },
        { "win.copy-magnet", { "<Primary>c", NULL } },
        { "win.resume", { "<Primary>r", NULL } },
        { "win.pause", { "<Primary>p", NULL } },
        { "win.verify", { "<Primary>h", NULL } },
        /* Re-announce also had Ctrl+Q in GTK 3, clashing with Quit */
        { "win.move", { "<Primary>m", NULL } },
        { "win.remove", { "Delete", NULL } },
        { "win.delete", { "<Shift>Delete", NULL } },
        { "win.resume-all", { "<Shift><Primary>r", NULL } },
        { "win.pause-all", { "<Shift><Primary>p", NULL } },
        { "win.local-prefs", { "<Primary>s", NULL } },
        { "win.remote-prefs", { "<Alt>s", NULL } },
        { "win.view-states", { "F2", NULL } },
        { "win.filter-dirs", { "F3", NULL } },
        { "win.filter-trackers", { "F4", NULL } },
        { "win.view-notebook", { "F5", NULL } },
        { "win.view-stats", { "F7", NULL } },
    };
    /* macOS conventions, and moving shortcuts that clash with the standard
     * ones: Command-C is Copy, Command-H Hide, Command-M Minimize and
     * Preferences is Command-comma (app.preferences, set by GTK). */
    static const trg_accel macos_accels[] = {
        { "win.copy-magnet", { "<Shift><Primary>c", NULL } },
        { "win.verify", { "<Shift><Primary>v", NULL } },
        { "win.move", { "<Shift><Primary>m", NULL } },
        { "win.remove", { "<Primary>BackSpace", "Delete", NULL } },
        { "win.delete", { "<Alt><Primary>BackSpace", "<Shift>Delete", NULL } },
        { "win.local-prefs", { NULL } },
        { "app.close-window", { "<Primary>w", NULL } },
        { "app.minimize", { "<Primary>m", NULL } },
    };
    gboolean macos = trg_is_macos();
    guint i;

    for (i = 0; i < G_N_ELEMENTS(accels); i++) {
        g_auto(GStrv) platform_accels = trg_menu_bar_platform_accels(accels[i].accels, macos);
        gtk_application_set_accels_for_action(app, accels[i].action,
                                              (const gchar *const *)platform_accels);
    }

    for (i = 0; macos && i < G_N_ELEMENTS(macos_accels); i++) {
        g_auto(GStrv) platform_accels = trg_menu_bar_platform_accels(macos_accels[i].accels, TRUE);
        gtk_application_set_accels_for_action(app, macos_accels[i].action,
                                              (const gchar *const *)platform_accels);
    }
}

TrgMenuBar *trg_menu_bar_new(TrgMainWindow *win, TrgPrefs *prefs, TrgTorrentTreeView *ttv)
{
    return g_object_new(TRG_TYPE_MENU_BAR, "torrent-tree-view", ttv, "prefs", prefs, "mainwin", win,
                        NULL);
}
