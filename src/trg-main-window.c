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

#include <gio/gio.h>
#include <glib.h>
#include <glib/gi18n.h>
#include <glib/gprintf.h>
#include <glib/gstdio.h>
#include <gtk/gtk.h>
#include <json-glib/json-glib.h>
#include <libsoup/soup.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "json.h"
#include "protocol-constants.h"
#include "requests.h"
#include "session-get.h"
#include "torrent.h"
#include "trg-client.h"
#include "util.h"

#include "trg-about-window.h"
#include "trg-files-model.h"
#include "trg-files-tree-view.h"
#include "trg-legacy-tree-view.h"
#include "trg-main-window.h"
#include "trg-menu-bar.h"
#include "trg-peers-model.h"
#include "trg-peers-tree-view.h"
#include "trg-preferences-dialog.h"
#include "trg-prefs.h"
#include "trg-remote-prefs-dialog.h"
#include "trg-state-selector.h"
#include "trg-stats-dialog.h"
#include "trg-status-bar.h"
#include "trg-toolbar.h"
#include "trg-torrent-add-dialog.h"
#include "trg-torrent-add-url-dialog.h"
#include "trg-torrent-model.h"
#include "trg-torrent-move-dialog.h"
#include "trg-torrent-props-dialog.h"
#include "trg-torrent-tree-view.h"
#include "trg-trackers-model.h"
#include "trg-trackers-tree-view.h"
#include "trg-tree-view.h"
#include "upload.h"

/* The rather large main window class, which glues everything together. */

static void update_selected_torrent_notebook(TrgMainWindow *win, gint mode, gint64 id);
static void torrent_event_notification(TrgTorrentModel *model, gchar *icon_name, gchar *desc,
                                       gchar *prefKey, TrgTorrentItem *item, gpointer data);
static void on_torrent_completed(TrgTorrentModel *model, TrgTorrentItem *item, gpointer data);
static void on_torrent_added(TrgTorrentModel *model, TrgTorrentItem *item, gpointer data);
static gboolean close_request(GtkWindow *w, gpointer data);
static void destroy_window(TrgMainWindow *win, gpointer data G_GNUC_UNUSED);
static void torrent_tv_onRowActivated(TrgTreeView *treeview, guint position, gpointer userdata);
static void add_url_cb(GSimpleAction *action, GVariant *parameter, gpointer data);
static void add_cb(GSimpleAction *action, GVariant *parameter, gpointer data);
static void disconnect_cb(GSimpleAction *action, GVariant *parameter, gpointer data);
static void open_local_prefs_cb(GSimpleAction *action, GVariant *parameter, gpointer data);
static void open_remote_prefs_cb(GSimpleAction *action, GVariant *parameter, gpointer data);
static void verify_cb(GSimpleAction *action, GVariant *parameter, gpointer data);
static void reannounce_cb(GSimpleAction *action, GVariant *parameter, gpointer data);
static void pause_cb(GSimpleAction *action, GVariant *parameter, gpointer data);
static void resume_cb(GSimpleAction *action, GVariant *parameter, gpointer data);
static void remove_cb(GSimpleAction *action, GVariant *parameter, gpointer data);
static void resume_all_cb(GSimpleAction *action, GVariant *parameter, gpointer data);
static void pause_all_cb(GSimpleAction *action, GVariant *parameter, gpointer data);
static void move_cb(GSimpleAction *action, GVariant *parameter, gpointer data);
static void delete_cb(GSimpleAction *action, GVariant *parameter, gpointer data);
static void open_props_cb(GSimpleAction *action, GVariant *parameter, gpointer data);
static void confirm_action_dialog(TrgMainWindow *win, const gchar *action_name,
                                  const gchar *action_label, JsonArray *ids, gboolean delete_data);
static void view_stats_toggled_cb(GSimpleAction *action, GVariant *parameter, gpointer data);
static void view_states_toggled_cb(GObject *action, GParamSpec *pspec, gpointer data);
static void view_notebook_toggled_cb(GObject *action, GParamSpec *pspec, gpointer data);
static GtkWidget *trg_main_window_notebook_new(TrgMainWindow *win);
static gboolean on_session_get_timer(gpointer data);
static gboolean on_session_get(gpointer data);
static gboolean on_torrent_get(gpointer data, int mode);
static gboolean on_torrent_get_first(gpointer data);
static gboolean on_torrent_get_active(gpointer data);
static gboolean on_torrent_get_update(gpointer data);
static gboolean on_torrent_get_interactive(gpointer data);
static gboolean trg_session_update_timerfunc(gpointer data);
static gboolean trg_update_torrents_timerfunc(gpointer data);
static void open_about_cb(GSimpleAction *action, GVariant *parameter, gpointer data);
static gboolean trg_torrent_tree_view_visible_func(gpointer item, gpointer data);
static gboolean trg_dialog_error_handler(TrgMainWindow *win, trg_response *response);
static void torrent_selection_changed(GtkSelectionModel *selection, guint position, guint n_items,
                                      TrgMainWindow *win);
static void trg_main_window_torrent_scrub(TrgMainWindow *win);
static void entry_filter_changed_cb(GtkWidget *w, TrgMainWindow *win);
static void torrent_state_selection_changed(TrgStateSelector *selector, guint flag, gpointer data);
static void trg_main_window_conn_changed(TrgMainWindow *win, gboolean connected);
static void trg_main_window_get_property(GObject *object, guint property_id, GValue *value,
                                         GParamSpec *pspec);
static void trg_main_window_set_property(GObject *object, guint property_id, const GValue *value,
                                         GParamSpec *pspec);
static TrgMenuBar *trg_main_window_menu_bar_new(TrgMainWindow *win);
static void clear_filter_entry_cb(GtkEntry *entry, GtkEntryIconPosition icon_pos,
                                  gpointer user_data);
static GMenuItem *trg_imagemenuitem_new(const gchar *text, const char *action,
                                        const char *icon_name);
static void set_limit_cb(GSimpleAction *action, GVariant *value, gpointer data);
static GMenuItem *limit_menu_new(TrgMainWindow *win, GActionMap *actions, const gchar *name,
                                 gchar *title, gchar *enabledKey, gchar *speedKey, JsonArray *ids);
static void trg_torrent_tv_view_menu(GtkWidget *treeview, gdouble x, gdouble y, TrgMainWindow *win);
static gboolean torrent_tv_context_menu_cb(GtkWidget *treeview, gdouble x, gdouble y,
                                           gpointer userdata);
static gboolean is_ready_for_torrent_action(TrgMainWindow *win);

struct _TrgMainWindow {
    GtkApplicationWindow parent;

    TrgClient *client;
    TrgToolbar *toolBar;
    TrgMenuBar *menuBar;

    TrgStatusBar *statusBar;
    TrgStateSelector *stateSelector;
    GtkWidget *stateSelectorScroller;
    TrgGeneralPanel *genDetails;
    GtkWidget *notebook;

    TrgTorrentModel *torrentModel;
    TrgTorrentTreeView *torrentTreeView;
    GtkFilter *torrentFilter;
    gint selectedTorrentId;

    /* what the torrent filter shows, from the state selector and the entry */
    guint32 filterCriteria;
    gchar *filterSelectedText;
    gchar *filterText;

    TrgTrackersModel *trackersModel;
    TrgTrackersTreeView *trackersTreeView;

    TrgFilesModel *filesModel;
    TrgFilesTreeView *filesTreeView;

    TrgPeersModel *peersModel;
    TrgPeersTreeView *peersTreeView;

    GtkWidget *hpaned, *vpaned;
    GtkWidget *filterEntry;

    guint timerId;
    guint sessionTimerId;
    gboolean queuesEnabled;
    gboolean stateSaved;

    gchar **args;
};

G_DEFINE_TYPE(TrgMainWindow, trg_main_window, GTK_TYPE_APPLICATION_WINDOW)

enum {
    PROP_0,
    PROP_CLIENT
};

static void reset_connect_args(TrgMainWindow *win)
{
    if (win->args) {
        g_strfreev(win->args);
        win->args = NULL;
    }
}

static void trg_main_window_init(TrgMainWindow *self)
{
}

gint trg_mw_get_selected_torrent_id(TrgMainWindow *win)
{
    return win->selectedTorrentId;
}

static void update_selected_torrent_notebook(TrgMainWindow *win, gint mode, gint64 id)
{
    TrgClient *client = win->client;
    gint64 serial = trg_client_get_serial(client);
    JsonObject *t;
    TrgTorrentItem *item;

    if (id >= 0 && get_torrent_data(trg_client_get_torrent_table(client), id, &t, &item)) {
        trg_menu_bar_torrent_actions_sensitive(win->menuBar, TRUE);
        trg_general_panel_update(win->genDetails, t, item);
        trg_trackers_model_update(win->trackersModel, serial, t, mode);
        trg_files_model_update(win->filesModel, GTK_TREE_VIEW(win->filesTreeView), serial, t, mode);
        trg_peers_model_update(win->peersModel, TRG_TREE_VIEW(win->peersTreeView), serial, t, mode);
    } else {
        trg_main_window_torrent_scrub(win);
    }

    win->selectedTorrentId = id;
}

static void torrent_event_notification(TrgTorrentModel *model, gchar *icon_name, gchar *desc,
                                       gchar *prefKey, TrgTorrentItem *item, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);
    TrgPrefs *prefs = trg_client_get_prefs(win->client);
    const gchar *name;
    GIcon *icon;
    GNotification *notify;

    if (!trg_prefs_get_bool(prefs, prefKey, TRG_PREFS_NOFLAGS))
        return;

    name = trg_item_get_string(item, TORRENT_COLUMN_NAME);

    icon = g_themed_icon_new(icon_name);
    notify = g_notification_new(name);
    g_notification_set_body(notify, desc);
    g_notification_set_icon(notify, icon);
    g_notification_set_priority(notify, G_NOTIFICATION_PRIORITY_LOW);
    g_application_send_notification(g_application_get_default(), name, notify);

    g_object_unref(notify);
    g_object_unref(icon);
}

static void on_torrent_completed(TrgTorrentModel *model, TrgTorrentItem *item, gpointer data)
{
    torrent_event_notification(model, "trg-gtk-apply", _("This torrent has completed."),
                               TRG_PREFS_KEY_COMPLETE_NOTIFY, item, data);
}

static void on_torrent_added(TrgTorrentModel *model, TrgTorrentItem *item, gpointer data)
{
    torrent_event_notification(model, "list-add", _("This torrent has been added."),
                               TRG_PREFS_KEY_ADD_NOTIFY, item, data);
}

static void save_window_size(TrgMainWindow *win)
{
    TrgPrefs *prefs = trg_client_get_prefs(win->client);
    gint width, height;

    gtk_window_get_default_size(GTK_WINDOW(win), &width, &height);

    trg_prefs_set_int(prefs, TRG_PREFS_KEY_WINDOW_HEIGHT, height, TRG_PREFS_GLOBAL);
    trg_prefs_set_int(prefs, TRG_PREFS_KEY_WINDOW_WIDTH, width, TRG_PREFS_GLOBAL);
}

/* Persist the window and tree view layout. This has to happen before the
 * window is destroyed, as its children are already gone by "destroy" in GTK 4. */
static void save_window_state(TrgMainWindow *win)
{
    TrgPrefs *prefs = trg_client_get_prefs(win->client);

    if (win->stateSaved)
        return;

    save_window_size(win);

    trg_prefs_set_int(prefs, TRG_PREFS_KEY_NOTEBOOK_PANED_POS,
                      gtk_paned_get_position(GTK_PANED(win->vpaned)), TRG_PREFS_GLOBAL);
    trg_prefs_set_int(prefs, TRG_PREFS_KEY_STATES_PANED_POS,
                      gtk_paned_get_position(GTK_PANED(win->hpaned)), TRG_PREFS_GLOBAL);

    trg_tree_view_persist(TRG_TREE_VIEW(win->peersTreeView),
                          TRG_TREE_VIEW_PERSIST_SORT | TRG_TREE_VIEW_PERSIST_LAYOUT);
    trg_legacy_tree_view_persist(TRG_LEGACY_TREE_VIEW(win->filesTreeView),
                                 TRG_LEGACY_TREE_VIEW_PERSIST_SORT
                                     | TRG_LEGACY_TREE_VIEW_PERSIST_LAYOUT);
    trg_tree_view_persist(TRG_TREE_VIEW(win->torrentTreeView),
                          TRG_TREE_VIEW_PERSIST_SORT | TRG_TREE_VIEW_PERSIST_LAYOUT);
    trg_tree_view_persist(TRG_TREE_VIEW(win->trackersTreeView),
                          TRG_TREE_VIEW_PERSIST_SORT | TRG_TREE_VIEW_PERSIST_LAYOUT);
    trg_prefs_save(prefs);

    win->stateSaved = TRUE;
}

static gboolean close_request(GtkWindow *w, gpointer data G_GNUC_UNUSED)
{
    save_window_state(TRG_MAIN_WINDOW(w));

    return FALSE;
}

static void destroy_window(TrgMainWindow *win, gpointer data G_GNUC_UNUSED)
{
    g_application_quit(g_application_get_default());
}

static void open_props_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);
    TrgTorrentPropsDialog *dialog;

    if (win->selectedTorrentId < 0)
        return;

    dialog = trg_torrent_props_dialog_new(GTK_WINDOW(win), win->torrentTreeView, win->torrentModel,
                                          win->client);

    gtk_window_present(GTK_WINDOW(dialog));
}

static void copy_magnetlink_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);
    JsonObject *json = NULL;
    GdkClipboard *clip = gtk_widget_get_clipboard(GTK_WIDGET(win));

    if (win->selectedTorrentId < 0)
        return;

    if (get_torrent_data(trg_client_get_torrent_table(win->client), win->selectedTorrentId, &json,
                         NULL))
        gdk_clipboard_set_text(clip, torrent_get_magnetlink(json));
}

static void torrent_tv_onRowActivated(TrgTreeView *treeview, guint position, gpointer userdata)
{
    open_props_cb(NULL, NULL, userdata);
}

static void add_url_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    TrgTorrentAddUrlDialog *dlg = trg_torrent_add_url_dialog_new(win, win->client);
    gtk_window_present(GTK_WINDOW(dlg));
}

static void add_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    if (trg_client_is_connected(win->client))
        trg_torrent_add_dialog(win, win->client);
}

static void pause_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);
    if (trg_client_is_connected(win->client))
        dispatch_rpc_async(win->client, torrent_pause(build_json_id_array(win->torrentTreeView)),
                           on_generic_interactive_action_response, win);
}

static void pause_all_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);
    if (trg_client_is_connected(win->client))
        dispatch_rpc_async(win->client, torrent_pause(NULL), on_generic_interactive_action_response,
                           win);
}

gint trg_add_from_filename(TrgMainWindow *win, gchar **uris)
{
    TrgClient *client = win->client;
    TrgPrefs *prefs = trg_client_get_prefs(client);
    GSList *filesList = NULL;
    int i;

    if (!trg_client_is_connected(client)) {
        g_strfreev(uris);
        return EXIT_SUCCESS;
    }

    if (uris) {
        for (i = 0; uris[i]; i++) {
            if (uris[i])
                filesList = g_slist_append(filesList, uris[i]);
        }
    }

    g_free(uris);

    if (!filesList)
        return EXIT_SUCCESS;

    if (trg_prefs_get_bool(prefs, TRG_PREFS_KEY_ADD_OPTIONS_DIALOG, TRG_PREFS_GLOBAL)) {
        TrgTorrentAddDialog *dialog
            = trg_torrent_add_dialog_new_from_filenames(win, client, filesList);

        gtk_window_present(GTK_WINDOW(dialog));
    } else {
        trg_upload *upload = g_new0(trg_upload, 1);

        upload->list = filesList;
        upload->main_window = win;
        upload->client = client;
        upload->extra_args = FALSE;
        upload->flags = trg_prefs_get_add_flags(prefs);

        trg_do_upload(upload);
    }

    return EXIT_SUCCESS;
}

static void resume_all_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);
    if (trg_client_is_connected(win->client))
        dispatch_rpc_async(win->client, torrent_start(NULL), on_generic_interactive_action_response,
                           win);
}

static void resume_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);
    if (trg_client_is_connected(win->client))
        dispatch_rpc_async(win->client, torrent_start(build_json_id_array(win->torrentTreeView)),
                           on_generic_interactive_action_response, win);
}

static void disconnect_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    trg_client_inc_connid(win->client);
    trg_main_window_conn_changed(TRG_MAIN_WINDOW(data), FALSE);
    trg_status_bar_reset(win->statusBar);
}

static void connect_error_response_cb(gpointer win)
{
    open_local_prefs_cb(NULL, NULL, win);
}

/* parameter is the index of the profile to connect to, or -1 for the current one */
void connect_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    TrgPrefs *prefs = trg_client_get_prefs(win->client);
    JsonObject *currentProfile = trg_prefs_get_profile(prefs);
    JsonObject *profile = NULL;
    g_autofree gchar *err_msg = NULL;

    if (parameter && g_variant_get_int32(parameter) >= 0) {
        JsonArray *profiles = trg_prefs_get_profiles(prefs);
        guint index = g_variant_get_int32(parameter);

        if (index < json_array_get_length(profiles))
            profile = json_array_get_object_element(profiles, index);
    }

    if (trg_client_is_connected(win->client))
        disconnect_cb(NULL, NULL, data);

    if (profile && currentProfile != profile)
        trg_prefs_set_profile(prefs, profile);
    else
        trg_prefs_profile_change_emit_signal(prefs);

    if (!trg_client_parse_settings(win->client, &err_msg)) {
        reset_connect_args(TRG_MAIN_WINDOW(data));

        /* open the preferences once the (modal) error is dismissed */
        trg_error_dialog_full(GTK_WINDOW(data), err_msg, connect_error_response_cb, win);

        return;
    }

    trg_status_bar_push_connection_msg(win->statusBar, _("Connecting..."));
    trg_client_inc_connid(win->client);
    dispatch_rpc_async(win->client, session_get(), on_session_get, data);
}

static void open_local_prefs_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);
    GtkWidget *dlg = trg_preferences_dialog_get_instance(win, win->client);
    gtk_window_present(GTK_WINDOW(dlg));
}

static void open_remote_prefs_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    if (trg_client_is_connected(win->client))
        gtk_window_present(GTK_WINDOW(trg_remote_prefs_dialog_get_instance(win, win->client)));
}

static gboolean action_get_active(GObject *action)
{
    g_autoptr(GVariant) state = g_action_get_state(G_ACTION(action));

    return g_variant_get_boolean(state);
}

static void main_window_toggle_filter_dirs(GObject *action, GParamSpec *pspec, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    if (g_action_get_enabled(G_ACTION(action)))
        trg_state_selector_set_show_dirs(win->stateSelector, action_get_active(action));
}

static void main_window_toggle_filter_trackers(GObject *action, GParamSpec *pspec, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    if (g_action_get_enabled(G_ACTION(action)))
        trg_state_selector_set_show_trackers(win->stateSelector, action_get_active(action));
}

static void main_window_toggle_directories_first(GObject *action, GParamSpec *pspec, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    if (g_action_get_enabled(G_ACTION(action)))
        trg_state_selector_set_directories_first(win->stateSelector, action_get_active(action));
}

static TrgToolbar *trg_main_window_toolbar_new(TrgMainWindow *win)
{
    TrgPrefs *prefs = trg_client_get_prefs(win->client);

    /* The toolbar buttons activate the same window actions as the menu. */
    return trg_toolbar_new(win, prefs);
}

static void reannounce_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    if (trg_client_is_connected(win->client))
        dispatch_rpc_async(win->client,
                           torrent_reannounce(build_json_id_array(win->torrentTreeView)),
                           on_generic_interactive_action_response, win);
}

static void verify_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    if (is_ready_for_torrent_action(win))
        dispatch_rpc_async(win->client, torrent_verify(build_json_id_array(win->torrentTreeView)),
                           on_generic_interactive_action_response, win);
}

static void start_now_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    if (is_ready_for_torrent_action(win))
        dispatch_rpc_async(win->client,
                           torrent_start_now(build_json_id_array(win->torrentTreeView)),
                           on_generic_interactive_action_response, win);
}

static void up_queue_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    if (win->queuesEnabled && is_ready_for_torrent_action(win))
        dispatch_rpc_async(win->client,
                           torrent_queue_move_up(build_json_id_array(win->torrentTreeView)),
                           on_generic_interactive_action_response, win);
}

static void top_queue_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    if (win->queuesEnabled && is_ready_for_torrent_action(win))
        dispatch_rpc_async(win->client,
                           torrent_queue_move_top(build_json_id_array(win->torrentTreeView)),
                           on_generic_interactive_action_response, win);
}

static void bottom_queue_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    if (win->queuesEnabled && is_ready_for_torrent_action(win))
        dispatch_rpc_async(win->client,
                           torrent_queue_move_bottom(build_json_id_array(win->torrentTreeView)),
                           on_generic_interactive_action_response, win);
}

static void down_queue_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    if (win->queuesEnabled && is_ready_for_torrent_action(win))
        dispatch_rpc_async(win->client,
                           torrent_queue_move_down(build_json_id_array(win->torrentTreeView)),
                           on_generic_interactive_action_response, win);
}

typedef struct {
    TrgMainWindow *win;
    JsonArray *ids;
    gboolean delete_data;
} confirm_action_data;

static void confirm_action_response_cb(GObject *source, GAsyncResult *result, gpointer user_data)
{
    confirm_action_data *data = user_data;
    TrgMainWindow *win = data->win;
    gint response = gtk_alert_dialog_choose_finish(GTK_ALERT_DIALOG(source), result, NULL);

    if (response == 1) {
        if (data->delete_data)
            dispatch_rpc_async(win->client, torrent_remove(data->ids, TRUE), on_delete_complete,
                               win);
        else
            dispatch_rpc_async(win->client, torrent_remove(data->ids, FALSE),
                               on_generic_interactive_action_response, win);
    } else {
        json_array_unref(data->ids);
    }

    g_free(data);
}

/* Asks for confirmation and removes the torrents in ids if given. Takes
 * ownership of ids. GTK 4 has no gtk_dialog_run(), so this is asynchronous. */
static void confirm_action_dialog(TrgMainWindow *win, const gchar *action_name,
                                  const gchar *action_label, JsonArray *ids, gboolean delete_data)
{
    const char *buttons[] = { _("_Cancel"), action_label, NULL };
    TrgTreeView *tv = TRG_TREE_VIEW(win->torrentTreeView);
    gint selectCount;
    GtkAlertDialog *dialog;
    gchar *markup, *message = NULL;
    confirm_action_data *data;

    selectCount = trg_tree_view_count_selected(tv);

    if (selectCount == 1) {
        markup = g_markup_printf_escaped(
            _("<big><b>%s \"%s\"?</b></big>"), action_name,
            trg_item_get_string(trg_tree_view_get_first_selected(tv), TORRENT_COLUMN_NAME));
    } else if (selectCount > 1) {
        markup = g_markup_printf_escaped(_("<big><b>%s %d torrents?</b></big>"), action_name,
                                         selectCount);
    } else {
        json_array_unref(ids);
        return;
    }

    /* The (translated) message is markup, an alert shows plain text */
    if (!pango_parse_markup(markup, -1, 0, NULL, &message, NULL, NULL))
        message = g_strdup(markup);
    dialog = gtk_alert_dialog_new("%s", message);
    g_free(message);
    g_free(markup);

    gtk_alert_dialog_set_buttons(dialog, buttons);
    gtk_alert_dialog_set_cancel_button(dialog, 0);
    gtk_alert_dialog_set_default_button(dialog, 0);
    gtk_alert_dialog_set_modal(dialog, TRUE);

    data = g_new0(confirm_action_data, 1);
    data->win = win;
    data->ids = ids;
    data->delete_data = delete_data;
    gtk_alert_dialog_choose(dialog, GTK_WINDOW(win), NULL, confirm_action_response_cb, data);
    g_object_unref(dialog);
}

static gboolean is_ready_for_torrent_action(TrgMainWindow *win)
{
    return win->selectedTorrentId >= 0 && trg_client_is_connected(win->client);
}

static void move_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    if (is_ready_for_torrent_action(win))
        gtk_window_present(
            GTK_WINDOW(trg_torrent_move_dialog_new(win, win->client, win->torrentTreeView)));
}

static void remove_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);
    JsonArray *ids;

    if (!is_ready_for_torrent_action(win))
        return;

    ids = build_json_id_array(win->torrentTreeView);

    confirm_action_dialog(win, _("Remove"), _("_Remove"), ids, FALSE);
}

static void delete_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);
    JsonArray *ids;

    if (!is_ready_for_torrent_action(win))
        return;

    ids = build_json_id_array(win->torrentTreeView);

    confirm_action_dialog(win, _("Remove and delete"), _("_Delete"), ids, TRUE);
}

static void view_stats_toggled_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    if (trg_client_is_connected(win->client)) {
        TrgStatsDialog *dlg = trg_stats_dialog_get_instance(TRG_MAIN_WINDOW(data), win->client);

        gtk_window_present(GTK_WINDOW(dlg));
    }
}

static void view_states_toggled_cb(GObject *action, GParamSpec *pspec, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    trg_widget_set_visible(win->stateSelectorScroller, action_get_active(action));
}

static void view_notebook_toggled_cb(GObject *action, GParamSpec *pspec, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    trg_widget_set_visible(win->notebook, action_get_active(action));
}

void trg_main_window_notebook_set_visible(TrgMainWindow *win, gboolean visible)
{
    trg_widget_set_visible(win->notebook, visible);
}

static GtkWidget *trg_main_window_notebook_new(TrgMainWindow *win)
{
    TrgPrefs *prefs = trg_client_get_prefs(win->client);

    GtkWidget *notebook = win->notebook = gtk_notebook_new();
    GtkWidget *genScrolledWin = gtk_scrolled_window_new();

    win->genDetails = trg_general_panel_new(win->client);
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(genScrolledWin), GTK_POLICY_AUTOMATIC,
                                   GTK_POLICY_AUTOMATIC);
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(genScrolledWin), GTK_WIDGET(win->genDetails));
    gtk_notebook_append_page(GTK_NOTEBOOK(notebook), genScrolledWin, gtk_label_new(_("General")));

    win->trackersModel = trg_trackers_model_new();
    win->trackersTreeView = trg_trackers_tree_view_new(win->trackersModel, win->client, win, NULL);
    gtk_notebook_append_page(GTK_NOTEBOOK(notebook), GTK_WIDGET(win->trackersTreeView),
                             gtk_label_new(_("Trackers")));

    win->filesModel = trg_files_model_new();
    win->filesTreeView = trg_files_tree_view_new(win->filesModel, win, win->client, NULL);
    gtk_notebook_append_page(GTK_NOTEBOOK(notebook),
                             my_scrolledwin_new(GTK_WIDGET(win->filesTreeView)),
                             gtk_label_new(_("Files")));

    win->peersModel = trg_peers_model_new();
    win->peersTreeView = trg_peers_tree_view_new(prefs, win->peersModel, NULL);
    gtk_notebook_append_page(GTK_NOTEBOOK(notebook), GTK_WIDGET(win->peersTreeView),
                             gtk_label_new(_("Peers")));

    return notebook;
}

gboolean on_session_set(gpointer data)
{
    trg_response *response = (trg_response *)data;
    TrgMainWindow *win = TRG_MAIN_WINDOW(response->cb_data);

    if (response->status == SOUP_STATUS_OK || response->status == FAIL_RESULT_UNSUCCESSFUL)
        trg_client_update_session(win->client, on_session_get, response->cb_data);

    trg_dialog_error_handler(TRG_MAIN_WINDOW(response->cb_data), response);
    trg_response_free(response);

    return FALSE;
}

static gboolean hasEnabledChanged(JsonObject *a, JsonObject *b, const gchar *key)
{
    return json_object_get_boolean_member(a, key) != json_object_get_boolean_member(b, key);
}

static gboolean on_session_get_timer(gpointer data)
{
    trg_response *response = (trg_response *)data;
    TrgMainWindow *win = TRG_MAIN_WINDOW(response->cb_data);
    TrgPrefs *prefs = trg_client_get_prefs(win->client);

    on_session_get(data);

    win->sessionTimerId = g_timeout_add_seconds(
        trg_prefs_get_int(prefs, TRG_PREFS_KEY_SESSION_UPDATE_INTERVAL, TRG_PREFS_CONNECTION),
        trg_session_update_timerfunc, win);

    return FALSE;
}

static gboolean on_session_get(gpointer data)
{
    trg_response *response = (trg_response *)data;
    TrgMainWindow *win = TRG_MAIN_WINDOW(response->cb_data);

    TrgClient *client = win->client;
    gboolean isConnected = trg_client_is_connected(client);
    JsonObject *lastSession = trg_client_get_session(client);
    JsonObject *newSession = NULL;

    if (response->obj)
        newSession = get_arguments(response->obj);

    if (!isConnected) {
        gdouble version;

        if (trg_dialog_error_handler(win, response)) {
            trg_response_free(response);
            reset_connect_args(win);
            return FALSE;
        }

        if ((version = session_get_version(newSession)) < TRANSMISSION_MIN_SUPPORTED) {
            gchar *msg = g_strdup_printf(
                _("This application supports Transmission %g and later, you have %g."),
                TRANSMISSION_MIN_SUPPORTED, version);
            trg_error_dialog(GTK_WINDOW(win), msg);
            g_free(msg);
            trg_response_free(response);
            reset_connect_args(win);
            return FALSE;
        }

        trg_status_bar_connect(win->statusBar, newSession, client);
    }

    if (newSession) {
        gboolean reloadAliases = lastSession
            && g_strcmp0(session_get_download_dir(lastSession),
                         session_get_download_dir(newSession));
        gboolean refreshSpeed = lastSession
            && (hasEnabledChanged(lastSession, newSession, SGET_ALT_SPEED_ENABLED)
                || hasEnabledChanged(lastSession, newSession, SGET_SPEED_LIMIT_DOWN_ENABLED)
                || hasEnabledChanged(lastSession, newSession, SGET_SPEED_LIMIT_UP_ENABLED));

        trg_client_set_session(client, newSession);

        if (reloadAliases)
            trg_main_window_reload_dir_aliases(win);

        if (refreshSpeed)
            trg_status_bar_update_speed(
                win->statusBar, trg_torrent_model_get_stats(win->torrentModel), win->client);
    }

    if (!isConnected) {
        trg_main_window_conn_changed(win, TRUE);
        trg_trackers_tree_view_new_connection(win->trackersTreeView, client);
        dispatch_rpc_async(client, torrent_get(TORRENT_GET_TAG_MODE_FULL), on_torrent_get_first,
                           win);
    }

    trg_response_free(response);

    return FALSE;
}

/*
 * The callback for a torrent-get response.
 */

static gboolean on_torrent_get(gpointer data, int mode)
{
    trg_response *response = (trg_response *)data;
    TrgMainWindow *win = TRG_MAIN_WINDOW(response->cb_data);
    TrgClient *client = win->client;
    TrgPrefs *prefs = trg_client_get_prefs(client);
    trg_torrent_model_update_stats *stats;
    guint interval;

    /* Disconnected between request and response callback */
    if (!trg_client_is_connected(client)) {
        trg_response_free(response);
        return FALSE;
    }

    interval = trg_prefs_get_int(prefs, TRG_PREFS_KEY_UPDATE_INTERVAL, TRG_PREFS_CONNECTION);
    if (interval < 1)
        interval = TRG_INTERVAL_DEFAULT;

    if (response->status != SOUP_STATUS_OK) {
        gint64 max_retries = trg_prefs_get_int(prefs, TRG_PREFS_KEY_RETRIES, TRG_PREFS_CONNECTION);

        if (trg_client_inc_failcount(client) >= max_retries) {
            trg_main_window_conn_changed(win, FALSE);
            trg_dialog_error_handler(win, response);
        } else {
            gchar *msg = make_error_message(response->obj, response->status, response->err_msg);
            gchar *statusBarMsg
                = g_strdup_printf(_("Request %d/%d failed: %s"), trg_client_get_failcount(client),
                                  (gint)max_retries, msg);
            trg_status_bar_push_connection_msg(win->statusBar, statusBarMsg);
            g_free(msg);
            g_free(statusBarMsg);
            win->timerId = g_timeout_add_seconds(interval, trg_update_torrents_timerfunc, win);
        }

        trg_response_free(response);

        return FALSE;
    }

    trg_client_reset_failcount(client);
    trg_client_inc_serial(client);

    stats = trg_torrent_model_update(win->torrentModel, client, response->obj, mode);

    /* The filter and sorter don't watch the items, so the values they look at
     * may have changed. */
    if (win->filterCriteria != 0 || win->filterText)
        gtk_filter_changed(win->torrentFilter, GTK_FILTER_CHANGE_DIFFERENT);
    trg_tree_view_resort(TRG_TREE_VIEW(win->torrentTreeView));

    update_selected_torrent_notebook(win, mode, win->selectedTorrentId);
    trg_status_bar_update(win->statusBar, stats, client);

    if (mode != TORRENT_GET_MODE_INTERACTION)
        win->timerId = g_timeout_add_seconds(interval, trg_update_torrents_timerfunc, win);

    trg_response_free(response);
    return FALSE;
}

static gboolean on_torrent_get_active(gpointer data)
{
    return on_torrent_get(data, TORRENT_GET_MODE_ACTIVE);
}

static gboolean on_torrent_get_first(gpointer data)
{
    trg_response *response = (trg_response *)data;
    TrgMainWindow *win = TRG_MAIN_WINDOW(response->cb_data);

    gboolean result = on_torrent_get(data, TORRENT_GET_MODE_FIRST);

    if (win->args) {
        trg_add_from_filename(win, win->args);
        win->args = NULL;
    }

    return result;
}

static gboolean on_torrent_get_interactive(gpointer data)
{
    return on_torrent_get(data, TORRENT_GET_MODE_INTERACTION);
}

static gboolean on_torrent_get_update(gpointer data)
{
    return on_torrent_get(data, TORRENT_GET_MODE_UPDATE);
}

static gboolean trg_session_update_timerfunc(gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    trg_client_update_session(win->client, on_session_get_timer, win);

    return FALSE;
}

static gboolean trg_update_torrents_timerfunc(gpointer data)
{
    /* Check if the TrgMainWindow* has already been destroyed
     * and, in that case, stop polling the server. */
    if (!TRG_IS_MAIN_WINDOW(data))
        return FALSE;

    TrgMainWindow *win = TRG_MAIN_WINDOW(data);
    TrgClient *tc = win->client;
    TrgPrefs *prefs = trg_client_get_prefs(tc);

    if (trg_client_is_connected(tc)) {
        gboolean activeOnly
            = trg_prefs_get_bool(prefs, TRG_PREFS_KEY_UPDATE_ACTIVE_ONLY, TRG_PREFS_CONNECTION)
            && (!trg_prefs_get_bool(prefs, TRG_PREFS_ACTIVEONLY_FULLSYNC_ENABLED,
                                    TRG_PREFS_CONNECTION)
                || (trg_client_get_serial(tc)
                        % trg_prefs_get_int(prefs, TRG_PREFS_ACTIVEONLY_FULLSYNC_EVERY,
                                            TRG_PREFS_CONNECTION)
                    != 0));
        dispatch_rpc_async(
            tc, torrent_get(activeOnly ? TORRENT_GET_TAG_MODE_UPDATE : TORRENT_GET_TAG_MODE_FULL),
            activeOnly ? on_torrent_get_active : on_torrent_get_update, data);
    }

    return FALSE;
}

static void open_about_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    GtkWidget *aboutDialog = trg_about_window_new(GTK_WINDOW(data));

    gtk_window_set_modal(GTK_WINDOW(aboutDialog), TRUE);
    gtk_window_set_hide_on_close(GTK_WINDOW(aboutDialog), FALSE);
    gtk_window_present(GTK_WINDOW(aboutDialog));
}

static gboolean trg_torrent_tree_view_visible_func(gpointer item, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);
    guint32 criteria = win->filterCriteria;

    if (criteria != 0) {
        if (criteria & FILTER_FLAG_TRACKER) {
            JsonObject *json = trg_torrent_item_get_json(item);

            if (!json
                || !torrent_has_tracker(json,
                                        trg_state_selector_get_url_host_regex(win->stateSelector),
                                        win->filterSelectedText))
                return FALSE;
        } else if (criteria & FILTER_FLAG_DIR) {
            if (g_strcmp0(win->filterSelectedText,
                          trg_item_get_string(item, TORRENT_COLUMN_DOWNLOADDIR_SHORT)))
                return FALSE;
        } else if (!(trg_item_get_uint(item, TORRENT_COLUMN_FLAGS) & criteria)) {
            return FALSE;
        }
    }

    if (win->filterText) {
        const gchar *name = trg_item_get_string(item, TORRENT_COLUMN_NAME);

        if (name) {
            gchar *nameCmp = g_utf8_casefold(name, -1);
            gboolean visible = strstr(nameCmp, win->filterText) != NULL;

            g_free(nameCmp);

            return visible;
        }
    }

    return TRUE;
}

/* The state selector or filter entry changed */
static void trg_main_window_refilter(TrgMainWindow *win)
{
    const gchar *text = gtk_editable_get_text(GTK_EDITABLE(win->filterEntry));

    win->filterCriteria = trg_state_selector_get_flag(win->stateSelector);

    g_free(win->filterSelectedText);
    win->filterSelectedText = win->filterCriteria & (FILTER_FLAG_TRACKER | FILTER_FLAG_DIR)
        ? trg_state_selector_get_selected_text(win->stateSelector)
        : NULL;

    g_free(win->filterText);
    win->filterText = text && *text ? g_utf8_casefold(text, -1) : NULL;

    gtk_filter_changed(win->torrentFilter, GTK_FILTER_CHANGE_DIFFERENT);
}

void trg_main_window_reload_dir_aliases(TrgMainWindow *win)
{
    trg_torrent_model_reload_dir_aliases(win->client, win->torrentModel);
}

static gboolean trg_dialog_error_handler(TrgMainWindow *win, trg_response *response)
{

    if (response->status != SOUP_STATUS_OK) {
        gchar *msg;

        msg = make_error_message(response->obj, response->status, response->err_msg);
        trg_status_bar_clear_indicators(win->statusBar);
        trg_status_bar_push_connection_msg(win->statusBar, msg);
        trg_error_dialog(GTK_WINDOW(win), msg);
        g_free(msg);
        return TRUE;
    } else {
        return FALSE;
    }
}

static gint64 trg_main_window_first_selected_id(TrgMainWindow *win)
{
    gpointer item = trg_tree_view_get_first_selected(TRG_TREE_VIEW(win->torrentTreeView));

    return item ? trg_torrent_item_get_id(item) : -1;
}

static void torrent_selection_changed(GtkSelectionModel *selection, guint position, guint n_items,
                                      TrgMainWindow *win)
{
    update_selected_torrent_notebook(win, TORRENT_GET_MODE_FIRST,
                                     trg_main_window_first_selected_id(win));
}

/* Torrents were added, removed, filtered or sorted. When the selected torrent
 * is gone, so is the selection, without a selection-changed. */
static void torrent_list_items_changed(GListModel *list, guint position, guint removed, guint added,
                                       TrgMainWindow *win)
{
    gint64 id = trg_main_window_first_selected_id(win);

    if (id != win->selectedTorrentId)
        update_selected_torrent_notebook(win, TORRENT_GET_MODE_FIRST, id);
}

gboolean on_delete_complete(gpointer data)
{
    trg_response *response = (trg_response *)data;
    TrgMainWindow *win = TRG_MAIN_WINDOW(response->cb_data);
    TrgClient *tc = win->client;

    if (trg_client_is_connected(tc) && response->status == SOUP_STATUS_OK)
        trg_client_update_session(win->client, on_session_get, response->cb_data);

    return on_generic_interactive_action_response(data);
}

void on_generic_interactive_action(TrgMainWindow *win, trg_response *response)
{
    TrgClient *tc = win->client;

    if (trg_client_is_connected(tc)) {
        trg_dialog_error_handler(win, response);

        if (response->status == SOUP_STATUS_OK) {
            gint64 id;
            if (json_object_has_member(response->obj, PARAM_TAG))
                id = json_object_get_int_member(response->obj, PARAM_TAG);
            else
                id = TORRENT_GET_TAG_MODE_FULL;

            dispatch_rpc_async(tc, torrent_get(id), on_torrent_get_interactive, win);
        }
    }

    trg_response_free(response);
}

gboolean on_generic_interactive_action_response(gpointer data)
{
    trg_response *response = (trg_response *)data;
    TrgMainWindow *win = TRG_MAIN_WINDOW(response->cb_data);

    on_generic_interactive_action(win, response);

    return FALSE;
}

static void trg_main_window_torrent_scrub(TrgMainWindow *win)
{

    gtk_tree_store_clear(GTK_TREE_STORE(win->filesModel));
    trg_trackers_model_clear(win->trackersModel);
    trg_peers_model_clear(win->peersModel);
    trg_general_panel_clear(win->genDetails);
    trg_trackers_model_set_no_selection(TRG_TRACKERS_MODEL(win->trackersModel));

    trg_menu_bar_torrent_actions_sensitive(win->menuBar, FALSE);
}

static void entry_filter_changed_cb(GtkWidget *w, TrgMainWindow *win)
{
    gboolean clearSensitive = gtk_entry_get_text_length(GTK_ENTRY(w)) > 0;

    trg_main_window_refilter(win);

    g_object_set(win->filterEntry, "secondary-icon-sensitive", clearSensitive, NULL);
}

static void torrent_state_selection_changed(TrgStateSelector *selector G_GNUC_UNUSED,
                                            guint flag G_GNUC_UNUSED, gpointer data)
{
    trg_main_window_refilter(TRG_MAIN_WINDOW(data));
}

static void trg_main_window_conn_changed(TrgMainWindow *win, gboolean connected)
{
    TrgClient *tc = win->client;

    trg_menu_bar_connected_change(win->menuBar, connected);

    gtk_widget_set_sensitive(GTK_WIDGET(win->torrentTreeView), connected);
    gtk_widget_set_sensitive(GTK_WIDGET(win->peersTreeView), connected);
    gtk_widget_set_sensitive(GTK_WIDGET(win->filesTreeView), connected);
    gtk_widget_set_sensitive(GTK_WIDGET(win->trackersTreeView), connected);
    gtk_widget_set_sensitive(GTK_WIDGET(win->genDetails), connected);

    if (connected) {
        TrgPrefs *prefs = trg_client_get_prefs(win->client);
        win->sessionTimerId = g_timeout_add_seconds(
            trg_prefs_get_int(prefs, TRG_PREFS_KEY_SESSION_UPDATE_INTERVAL, TRG_PREFS_CONNECTION),
            trg_session_update_timerfunc, win);
    } else {
        trg_main_window_torrent_scrub(win);
        trg_state_selector_disconnect(win->stateSelector);

        trg_torrent_model_remove_all(win->torrentModel);
        g_clear_handle_id(&win->timerId, g_source_remove);
        g_clear_handle_id(&win->sessionTimerId, g_source_remove);
    }

    trg_client_status_change(tc, connected);
}

static void trg_main_window_get_property(GObject *object, guint property_id, GValue *value,
                                         GParamSpec *pspec)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(object);

    switch (property_id) {
    case PROP_CLIENT:
        g_value_set_pointer(value, win->client);
        break;
    default:
        G_OBJECT_WARN_INVALID_PROPERTY_ID(object, property_id, pspec);
        break;
    }
}

static void trg_main_window_set_property(GObject *object, guint property_id, const GValue *value,
                                         GParamSpec *pspec)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(object);

    switch (property_id) {
    case PROP_CLIENT:
        win->client = g_value_get_pointer(value);
        break;
    default:
        G_OBJECT_WARN_INVALID_PROPERTY_ID(object, property_id, pspec);
        break;
    }
}

static const GActionEntry win_entries[] = {
    /* parameter: profile index, -1 for the current profile. state: current profile */
    { "connect", connect_cb, "i", "-1", NULL, { 0 } },
    { "disconnect", disconnect_cb, NULL, NULL, NULL, { 0 } },
    { "add", add_cb, NULL, NULL, NULL, { 0 } },
    { "add-url", add_url_cb, NULL, NULL, NULL, { 0 } },
    { "properties", open_props_cb, NULL, NULL, NULL, { 0 } },
    { "copy-magnet", copy_magnetlink_cb, NULL, NULL, NULL, { 0 } },
    { "resume", resume_cb, NULL, NULL, NULL, { 0 } },
    { "pause", pause_cb, NULL, NULL, NULL, { 0 } },
    { "verify", verify_cb, NULL, NULL, NULL, { 0 } },
    { "reannounce", reannounce_cb, NULL, NULL, NULL, { 0 } },
    { "move", move_cb, NULL, NULL, NULL, { 0 } },
    { "remove", remove_cb, NULL, NULL, NULL, { 0 } },
    { "delete", delete_cb, NULL, NULL, NULL, { 0 } },
    { "start-now", start_now_cb, NULL, NULL, NULL, { 0 } },
    { "up-queue", up_queue_cb, NULL, NULL, NULL, { 0 } },
    { "down-queue", down_queue_cb, NULL, NULL, NULL, { 0 } },
    { "bottom-queue", bottom_queue_cb, NULL, NULL, NULL, { 0 } },
    { "top-queue", top_queue_cb, NULL, NULL, NULL, { 0 } },
    { "resume-all", resume_all_cb, NULL, NULL, NULL, { 0 } },
    { "pause-all", pause_all_cb, NULL, NULL, NULL, { 0 } },
    { "local-prefs", open_local_prefs_cb, NULL, NULL, NULL, { 0 } },
    { "remote-prefs", open_remote_prefs_cb, NULL, NULL, NULL, { 0 } },
    { "view-stats", view_stats_toggled_cb, NULL, NULL, NULL, { 0 } },
    { "about", open_about_cb, NULL, NULL, NULL, { 0 } },
};

static void trg_main_window_action_notify(TrgMainWindow *win, const gchar *name, GCallback cb)
{
    GAction *action = g_action_map_lookup_action(G_ACTION_MAP(win), name);

    g_signal_connect(action, "notify::state", cb, win);
}

static TrgMenuBar *trg_main_window_menu_bar_new(TrgMainWindow *win)
{
    TrgMenuBar *menuBar;

    /* Everything the menus and toolbar can do is a "win." action. */
    g_action_map_add_action_entries(G_ACTION_MAP(win), win_entries, G_N_ELEMENTS(win_entries), win);

    /* This also adds the stateful view actions (check/radio items). */
    menuBar = trg_menu_bar_new(win, trg_client_get_prefs(win->client), win->torrentTreeView);

    trg_main_window_action_notify(win, "view-notebook", G_CALLBACK(view_notebook_toggled_cb));
    trg_main_window_action_notify(win, "filter-dirs", G_CALLBACK(main_window_toggle_filter_dirs));
    trg_main_window_action_notify(win, "filter-trackers",
                                  G_CALLBACK(main_window_toggle_filter_trackers));
    trg_main_window_action_notify(win, "directories-first",
                                  G_CALLBACK(main_window_toggle_directories_first));
    trg_main_window_action_notify(win, "view-states", G_CALLBACK(view_states_toggled_cb));

    return menuBar;
}

static void clear_filter_entry_cb(GtkEntry *entry, GtkEntryIconPosition icon_pos,
                                  gpointer user_data)
{
    gtk_editable_set_text(GTK_EDITABLE(entry), "");
}

static GMenuItem *trg_imagemenuitem_new(const gchar *text, const char *action,
                                        const char *icon_name)
{
    GMenuItem *item = g_menu_item_new(text, action);
    GIcon *icon = g_themed_icon_new(icon_name);

    g_menu_item_set_icon(item, icon);
    g_object_unref(icon);

    return item;
}

static void trg_menu_append_image_item(GMenu *menu, const gchar *text, const char *action,
                                       const char *icon_name)
{
    GMenuItem *item = trg_imagemenuitem_new(text, action, icon_name);

    g_menu_append_item(menu, item);
    g_object_unref(item);
}

/* The speed limit and priority items of the context menu are radio items of
 * stateful actions in a per-menu action group ("torrent."). The keys and
 * torrent ids are attached to the action. */
static void set_limit_cb(GSimpleAction *action, GVariant *value, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);
    gint speed;
    gchar *speedKey, *enabledKey;
    gpointer limitIds;
    JsonNode *req = NULL;
    JsonObject *args;

    g_simple_action_set_state(action, value);

    speed = g_variant_get_int32(value);
    speedKey = g_object_get_data(G_OBJECT(action), "speedKey");
    enabledKey = g_object_get_data(G_OBJECT(action), "enabledKey");
    limitIds = g_object_get_data(G_OBJECT(action), "limit-ids");

    if (limitIds)
        req = torrent_set(json_array_ref((JsonArray *)limitIds));
    else
        req = session_set();

    args = node_get_arguments(req);

    if (speed >= 0)
        json_object_set_int_member(args, speedKey, speed);

    json_object_set_boolean_member(args, enabledKey, speed >= 0);

    if (limitIds)
        dispatch_rpc_async(win->client, req, on_generic_interactive_action_response, win);
    else
        dispatch_rpc_async(win->client, req, on_session_set, win);
}

static void set_priority_cb(GSimpleAction *action, GVariant *value, gpointer data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(data);

    gint priority = g_variant_get_int32(value);
    gpointer limitIds = g_object_get_data(G_OBJECT(action), "pri-ids");

    JsonNode *req = NULL;
    JsonObject *args;

    g_simple_action_set_state(action, value);

    req = torrent_set(json_array_ref((JsonArray *)limitIds));

    args = node_get_arguments(req);

    json_object_set_int_member(args, FIELD_BANDWIDTH_PRIORITY, priority);

    dispatch_rpc_async(win->client, req, on_generic_interactive_action_response, win);
}

static void limit_item_new(GMenu *menu, const gchar *detailed_action, gfloat limit)
{
    char speed[32];
    GMenuItem *item;

    trg_strlspeed(speed, limit);

    item = g_menu_item_new(speed, NULL);
    g_menu_item_set_action_and_target(item, detailed_action, "i", (gint)limit);
    g_menu_append_item(menu, item);
    g_object_unref(item);
}

static void priority_menu_item_new(GMenu *menu, const gchar *label, gint value)
{
    GMenuItem *item = g_menu_item_new(label, NULL);

    g_menu_item_set_action_and_target(item, "torrent.priority", "i", value);
    g_menu_append_item(menu, item);
    g_object_unref(item);
}

static GMenuItem *priority_menu_new(TrgMainWindow *win, GActionMap *actions, JsonArray *ids)
{
    TrgClient *client = win->client;
    JsonObject *t = NULL;
    gint selected_pri = TR_PRI_UNSET;
    GSimpleAction *action;
    GMenuItem *toplevel;
    GMenu *menu;
    GIcon *icon;

    if (get_torrent_data(trg_client_get_torrent_table(client), win->selectedTorrentId, &t, NULL))
        selected_pri = torrent_get_bandwidth_priority(t);

    action = g_simple_action_new_stateful("priority", G_VARIANT_TYPE_INT32,
                                          g_variant_new_int32(selected_pri));
    g_object_set_data_full(G_OBJECT(action), "pri-ids", json_array_ref(ids),
                           (GDestroyNotify)json_array_unref);
    g_signal_connect(action, "change-state", G_CALLBACK(set_priority_cb), win);
    g_action_map_add_action(actions, G_ACTION(action));
    g_object_unref(action);

    menu = g_menu_new();

    priority_menu_item_new(menu, _("High"), TR_PRI_HIGH);
    priority_menu_item_new(menu, _("Normal"), TR_PRI_NORMAL);
    priority_menu_item_new(menu, _("Low"), TR_PRI_LOW);

    toplevel = g_menu_item_new_submenu(_("Priority"), G_MENU_MODEL(menu));
    icon = g_themed_icon_new("network-workgroup");
    g_menu_item_set_icon(toplevel, icon);
    g_object_unref(icon);
    g_object_unref(menu);

    return toplevel;
}

static GMenuItem *limit_menu_new(TrgMainWindow *win, GActionMap *actions, const gchar *name,
                                 gchar *title, gchar *enabledKey, gchar *speedKey, JsonArray *ids)
{
    TrgClient *client = win->client;
    JsonObject *current = NULL;
    GMenuItem *toplevel, *item;
    GMenu *menu, *section;
    GSimpleAction *action;
    GIcon *icon;
    gchar *detailed;
    gint64 limit;
    gboolean is_custom;
    gint64 v;
    gint i;

    if (ids)
        get_torrent_data(trg_client_get_torrent_table(client), win->selectedTorrentId, &current,
                         NULL);
    else
        current = trg_client_get_session(client);

    limit = json_object_get_boolean_member(current, enabledKey)
        ? json_object_get_int_member(current, speedKey)
        : -1;

    action = g_simple_action_new_stateful(name, G_VARIANT_TYPE_INT32,
                                          g_variant_new_int32((gint)limit));
    g_object_set_data_full(G_OBJECT(action), "speedKey", g_strdup(speedKey), g_free);
    g_object_set_data_full(G_OBJECT(action), "enabledKey", g_strdup(enabledKey), g_free);
    if (ids)
        g_object_set_data_full(G_OBJECT(action), "limit-ids", json_array_ref(ids),
                               (GDestroyNotify)json_array_unref);
    g_signal_connect(action, "change-state", G_CALLBACK(set_limit_cb), win);
    g_action_map_add_action(actions, G_ACTION(action));
    g_object_unref(action);

    detailed = g_strconcat("torrent.", name, NULL);

    menu = g_menu_new();
    section = g_menu_new();

    item = g_menu_item_new(_("No Limit"), NULL);
    g_menu_item_set_action_and_target(item, detailed, "i", -1);
    g_menu_append_item(section, item);
    g_object_unref(item);

    /* bandwidth items: n = 5*2^(n-1) */
    if (limit >= 0) {
        is_custom = TRUE;
        v = 5;
        for (i = 0; i < 16; i++) {
            if (limit == v) {
                is_custom = FALSE;
                break;
            }
            v *= 2;
        }
        if (is_custom) {
            /* GTK 3 showed this checked but insensitive. A disabled action
             * doesn't show its state in a GTK 4 menu, so this is a normal
             * (checked) radio item, choosing it sets the same limit again. */
            char speed[32];
            trg_strlspeed(speed, limit);
            item = g_menu_item_new(speed, NULL);
            g_menu_item_set_action_and_target(item, detailed, "i", (gint)limit);
            g_menu_append_item(section, item);
            g_object_unref(item);
        }
    }
    g_menu_append_section(menu, NULL, G_MENU_MODEL(section));
    g_object_unref(section);

    section = g_menu_new();
    v = 5;
    for (i = 0; i < 16; i++) {
        limit_item_new(section, detailed, v);
        v *= 2;
    }
    g_menu_append_section(menu, NULL, G_MENU_MODEL(section));
    g_object_unref(section);

    toplevel = g_menu_item_new_submenu(title, G_MENU_MODEL(menu));
    icon = g_themed_icon_new("network-workgroup");
    g_menu_item_set_icon(toplevel, icon);
    g_object_unref(icon);

    g_object_unref(menu);
    g_free(detailed);

    return toplevel;
}

static void trg_torrent_tv_view_menu(GtkWidget *treeview, gdouble x, gdouble y, TrgMainWindow *win)
{
    GSimpleActionGroup *actions = g_simple_action_group_new();
    GMenu *menu, *section;
    GMenuItem *item;
    JsonArray *ids;

    menu = g_menu_new();

    ids = build_json_id_array(TRG_TORRENT_TREE_VIEW(treeview));

    section = g_menu_new();
    trg_menu_append_image_item(section, _("Properties"), "win.properties", "document-properties");
    trg_menu_append_image_item(section, _("Copy Magnet Link"), "win.copy-magnet", "edit-copy");
    trg_menu_append_image_item(section, _("Resume"), "win.resume", "media-playback-start");
    trg_menu_append_image_item(section, _("Pause"), "win.pause", "media-playback-pause");
    trg_menu_append_image_item(section, _("Verify"), "win.verify", "view-refresh");
    trg_menu_append_image_item(section, _("Re-announce"), "win.reannounce", "network-server");
    trg_menu_append_image_item(section, _("Move"), "win.move", "drive-harddisk");
    trg_menu_append_image_item(section, _("Remove"), "win.remove", "list-remove");
    trg_menu_append_image_item(section, _("Remove and delete data"), "win.delete", "edit-delete");
    g_menu_append_section(menu, NULL, G_MENU_MODEL(section));
    g_object_unref(section);

    if (win->queuesEnabled) {
        section = g_menu_new();
        trg_menu_append_image_item(section, _("Start Now"), "win.start-now",
                                   "media-playback-start");
        trg_menu_append_image_item(section, _("Move Up Queue"), "win.up-queue", "go-up");
        trg_menu_append_image_item(section, _("Move Down Queue"), "win.down-queue", "go-down");
        trg_menu_append_image_item(section, _("Bottom Of Queue"), "win.bottom-queue", "go-bottom");
        trg_menu_append_image_item(section, _("Top Of Queue"), "win.top-queue", "go-top");
        g_menu_append_section(menu, NULL, G_MENU_MODEL(section));
        g_object_unref(section);
    }

    section = g_menu_new();
    item = limit_menu_new(win, G_ACTION_MAP(actions), "down-limit", _("Down Limit"),
                          FIELD_DOWNLOAD_LIMITED, FIELD_DOWNLOAD_LIMIT, ids);
    g_menu_append_item(section, item);
    g_object_unref(item);
    item = limit_menu_new(win, G_ACTION_MAP(actions), "up-limit", _("Up Limit"),
                          FIELD_UPLOAD_LIMITED, FIELD_UPLOAD_LIMIT, ids);
    g_menu_append_item(section, item);
    g_object_unref(item);
    item = priority_menu_new(win, G_ACTION_MAP(actions), ids);
    g_menu_append_item(section, item);
    g_object_unref(item);
    g_menu_append_section(menu, NULL, G_MENU_MODEL(section));
    g_object_unref(section);

    json_array_unref(ids);

    trg_popover_menu_popup(treeview, G_MENU_MODEL(menu), x, y, "torrent", G_ACTION_GROUP(actions));

    g_object_unref(menu);
    g_object_unref(actions);
}

static gboolean torrent_tv_context_menu_cb(GtkWidget *treeview, gdouble x, gdouble y,
                                           gpointer userdata)
{
    TrgTreeView *tv = TRG_TREE_VIEW(treeview);

    /* from the keyboard */
    if (x < 0) {
        trg_torrent_tv_view_menu(treeview, x, y, userdata);
        return TRUE;
    }

    /* the column headers have their own menu */
    if (trg_tree_view_point_in_header(tv, x, y))
        return FALSE;

    if (trg_tree_view_select_at(tv, x, y)) {
        trg_torrent_tv_view_menu(treeview, x, y, userdata);
        return TRUE;
    }

    return FALSE;
}

TrgStateSelector *trg_main_window_get_state_selector(TrgMainWindow *win)
{
    return win->stateSelector;
}

static void trg_client_session_updated_cb(TrgClient *tc, JsonObject *session, TrgMainWindow *win)
{
    gboolean queuesEnabled;

    trg_status_bar_session_update(win->statusBar, session);

    if (json_object_has_member(session, SGET_DOWNLOAD_QUEUE_ENABLED)) {
        queuesEnabled = json_object_get_boolean_member(session, SGET_DOWNLOAD_QUEUE_ENABLED)
            || json_object_get_boolean_member(session, SGET_SEED_QUEUE_ENABLED);
    } else {
        queuesEnabled = FALSE;
    }

    if (win->queuesEnabled != queuesEnabled) {
        trg_menu_bar_set_supports_queues(win->menuBar, queuesEnabled);
        trg_state_selector_set_queues_enabled(win->stateSelector, queuesEnabled);
    }

    win->queuesEnabled = queuesEnabled;
}

/* Drag & Drop support */
static gboolean on_dropped_file(GtkDropTarget *target, const GValue *value, gdouble x, gdouble y,
                                gpointer user_data)
{
    TrgMainWindow *win = user_data;
    GSList *files, *li;
    gchar **file_list;
    guint i = 0;

    if (!G_VALUE_HOLDS(value, GDK_TYPE_FILE_LIST))
        return FALSE;

    files = g_value_get_boxed(value);
    file_list = g_new0(gchar *, g_slist_length(files) + 1);

    for (li = files; li; li = g_slist_next(li)) {
        gchar *path = g_file_get_path(G_FILE(li->data));
        if (path)
            file_list[i++] = path;
    }

    trg_add_from_filename(win, file_list);

    return TRUE;
}

static gboolean window_key_press_handler(GtkEventControllerKey *controller, guint keyval,
                                         guint keycode, GdkModifierType state, gpointer user_data)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(user_data);
    GdkModifierType primary = trg_is_macos() ? GDK_META_MASK : GDK_CONTROL_MASK;

    if ((state & primary) && (keyval == GDK_KEY_k || keyval == GDK_KEY_f)) {
        gtk_widget_grab_focus(win->filterEntry);
        return TRUE;
    }

    return FALSE;
}

static GObject *trg_main_window_constructor(GType type, guint n_construct_properties,
                                            GObjectConstructParam *construct_params)
{
    TrgMainWindow *self
        = TRG_MAIN_WINDOW(G_OBJECT_CLASS(trg_main_window_parent_class)
                              ->constructor(type, n_construct_properties, construct_params));
    GtkWidget *w;
    GtkWidget *outerVbox;
    GtkWidget *toolbarHbox;
    // GtkWidget *outerAlignment;
    GtkEventController *keyController;
    GtkDropTarget *dropTarget;
    GtkFilterListModel *filtered;
    GtkSelectionModel *selection;
    gint width, height, pos;
    TrgPrefs *prefs;

    self->queuesEnabled = TRUE;

    prefs = trg_client_get_prefs(self->client);

    gtk_window_set_default_icon_name(PACKAGE_NAME);

    gtk_window_set_title(GTK_WINDOW(self), _("Transmission Remote"));
    gtk_window_set_default_size(GTK_WINDOW(self), 1000, 600);

    g_signal_connect(G_OBJECT(self), "close-request", G_CALLBACK(close_request), NULL);
    g_signal_connect(G_OBJECT(self), "destroy", G_CALLBACK(destroy_window), NULL);

    /* Ctrl+K/Ctrl+F (Command-K/F on macOS), before the focus widget gets
     * them like "key-press-event" did */
    keyController = gtk_event_controller_key_new();
    gtk_event_controller_set_propagation_phase(keyController, GTK_PHASE_CAPTURE);
    g_signal_connect(keyController, "key-pressed", G_CALLBACK(window_key_press_handler), self);
    gtk_widget_add_controller(GTK_WIDGET(self), keyController);

    self->torrentModel = trg_torrent_model_new();
    trg_client_set_torrent_table(self->client, get_torrent_table(self->torrentModel));

    g_signal_connect(self->torrentModel, "torrent-completed", G_CALLBACK(on_torrent_completed),
                     self);
    g_signal_connect(self->torrentModel, "torrent-added", G_CALLBACK(on_torrent_added), self);

    /* the torrents shown, filtered by the state selector and the filter entry */
    self->torrentFilter
        = GTK_FILTER(gtk_custom_filter_new(trg_torrent_tree_view_visible_func, self, NULL));
    filtered = gtk_filter_list_model_new(G_LIST_MODEL(g_object_ref(self->torrentModel)),
                                         g_object_ref(self->torrentFilter));

    self->torrentTreeView = trg_torrent_tree_view_new(self->client, G_LIST_MODEL(filtered));
    g_object_unref(filtered);
    selection = trg_tree_view_get_selection(TRG_TREE_VIEW(self->torrentTreeView));
    g_signal_connect(selection, "selection-changed", G_CALLBACK(torrent_selection_changed), self);
    g_signal_connect(selection, "items-changed", G_CALLBACK(torrent_list_items_changed), self);
    trg_widget_add_context_menu(GTK_WIDGET(self->torrentTreeView), torrent_tv_context_menu_cb,
                                self);
    g_signal_connect(self->torrentTreeView, "row-activated", G_CALLBACK(torrent_tv_onRowActivated),
                     self);

    outerVbox = trg_vbox_new(FALSE, 0);

    gtk_window_set_child(GTK_WINDOW(self), outerVbox);

    /* The menu is a GMenuModel, shown here with a GtkPopoverMenuBar, or in
     * the macOS menu bar (see trg_gtk_app_startup()). */
    self->menuBar = trg_main_window_menu_bar_new(self);
    if (!trg_is_macos())
        gtk_box_append(GTK_BOX(outerVbox),
                       gtk_popover_menu_bar_new_from_model(trg_menu_bar_get_model(self->menuBar)));

    toolbarHbox = trg_hbox_new(FALSE, 0);
    self->toolBar = trg_main_window_toolbar_new(self);
    gtk_widget_set_hexpand(GTK_WIDGET(self->toolBar), TRUE);
    gtk_box_append(GTK_BOX(toolbarHbox), GTK_WIDGET(self->toolBar));

    w = gtk_entry_new();
    gtk_entry_set_icon_from_icon_name(GTK_ENTRY(w), GTK_ENTRY_ICON_SECONDARY, "edit-clear");
    g_signal_connect(w, "icon-release", G_CALLBACK(clear_filter_entry_cb), NULL);
    gtk_widget_set_valign(w, GTK_ALIGN_CENTER);
    gtk_box_append(GTK_BOX(toolbarHbox), w);
    g_object_set(w, "secondary-icon-sensitive", FALSE, NULL);
    self->filterEntry = w;

    g_signal_connect(G_OBJECT(self->filterEntry), "changed", G_CALLBACK(entry_filter_changed_cb),
                     self);

    gtk_box_append(GTK_BOX(outerVbox), GTK_WIDGET(toolbarHbox));

    self->hpaned = gtk_paned_new(GTK_ORIENTATION_HORIZONTAL);
    self->vpaned = gtk_paned_new(GTK_ORIENTATION_VERTICAL);

    gtk_widget_set_vexpand(self->vpaned, TRUE);
    gtk_box_append(GTK_BOX(outerVbox), self->vpaned);
    gtk_paned_set_start_child(GTK_PANED(self->vpaned), self->hpaned);
    gtk_paned_set_resize_start_child(GTK_PANED(self->vpaned), TRUE);
    gtk_paned_set_shrink_start_child(GTK_PANED(self->vpaned), TRUE);

    self->stateSelector = trg_state_selector_new(self->client, self->torrentModel);
    self->stateSelectorScroller = my_scrolledwin_new(GTK_WIDGET(self->stateSelector));
    gtk_paned_set_start_child(GTK_PANED(self->hpaned), self->stateSelectorScroller);
    gtk_paned_set_resize_start_child(GTK_PANED(self->hpaned), FALSE);
    gtk_paned_set_shrink_start_child(GTK_PANED(self->hpaned), FALSE);

    gtk_paned_set_end_child(GTK_PANED(self->hpaned), GTK_WIDGET(self->torrentTreeView));
    gtk_paned_set_resize_end_child(GTK_PANED(self->hpaned), TRUE);
    gtk_paned_set_shrink_end_child(GTK_PANED(self->hpaned), TRUE);

    g_signal_connect(G_OBJECT(self->stateSelector), "torrent-state-changed",
                     G_CALLBACK(torrent_state_selection_changed), self);

    self->notebook = trg_main_window_notebook_new(self);
    gtk_paned_set_end_child(GTK_PANED(self->vpaned), self->notebook);
    gtk_paned_set_resize_end_child(GTK_PANED(self->vpaned), FALSE);
    gtk_paned_set_shrink_end_child(GTK_PANED(self->vpaned), FALSE);

    /* There's no system tray (TRG_PREFS_KEY_SYSTEM_TRAY) in GTK 4 */

    self->statusBar = trg_status_bar_new(self, self->client);
    g_signal_connect(self->client, "session-updated", G_CALLBACK(trg_client_session_updated_cb),
                     self);

    gtk_widget_set_margin_top(GTK_WIDGET(self->statusBar), 2);
    gtk_widget_set_margin_bottom(GTK_WIDGET(self->statusBar), 2);
    gtk_box_append(GTK_BOX(outerVbox), GTK_WIDGET(self->statusBar));

    width = trg_prefs_get_int(prefs, TRG_PREFS_KEY_WINDOW_WIDTH, TRG_PREFS_GLOBAL);
    height = trg_prefs_get_int(prefs, TRG_PREFS_KEY_WINDOW_HEIGHT, TRG_PREFS_GLOBAL);

    pos = trg_prefs_get_int(prefs, TRG_PREFS_KEY_NOTEBOOK_PANED_POS, TRG_PREFS_GLOBAL);

    if (width > 0 && height > 0)
        gtk_window_set_default_size(GTK_WINDOW(self), width, height);
    else if (pos < 1)
        gtk_paned_set_position(GTK_PANED(self->vpaned), 300);

    if (pos > 0)
        gtk_paned_set_position(GTK_PANED(self->vpaned), pos);

    trg_widget_set_visible(
        self->stateSelectorScroller,
        trg_prefs_get_bool(prefs, TRG_PREFS_KEY_SHOW_STATE_SELECTOR, TRG_PREFS_GLOBAL));
    trg_widget_set_visible(
        self->notebook, trg_prefs_get_bool(prefs, TRG_PREFS_KEY_SHOW_NOTEBOOK, TRG_PREFS_GLOBAL));

    pos = trg_prefs_get_int(prefs, TRG_PREFS_KEY_STATES_PANED_POS, TRG_PREFS_GLOBAL);
    if (pos > 0)
        gtk_paned_set_position(GTK_PANED(self->hpaned), pos);

    /* Drag and Drop */
    dropTarget = gtk_drop_target_new(GDK_TYPE_FILE_LIST, GDK_ACTION_COPY | GDK_ACTION_MOVE);
    g_signal_connect(dropTarget, "drop", G_CALLBACK(on_dropped_file), self);
    gtk_widget_add_controller(GTK_WIDGET(self), GTK_EVENT_CONTROLLER(dropTarget));

    return G_OBJECT(self);
}

static void trg_main_window_finalize(GObject *object)
{
    TrgMainWindow *win = TRG_MAIN_WINDOW(object);

    g_clear_object(&win->torrentFilter);
    g_free(win->filterSelectedText);
    g_free(win->filterText);

    G_OBJECT_CLASS(trg_main_window_parent_class)->finalize(object);
}

static void trg_main_window_class_init(TrgMainWindowClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);

    object_class->constructor = trg_main_window_constructor;
    object_class->finalize = trg_main_window_finalize;
    object_class->get_property = trg_main_window_get_property;
    object_class->set_property = trg_main_window_set_property;

    g_object_class_install_property(
        object_class, PROP_CLIENT,
        g_param_spec_pointer("trg-client", "TClient", "Client",
                             G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_STRINGS));
}

void trg_main_window_set_start_args(TrgMainWindow *win, gchar **args)
{
    win->args = args;
}

/* Adds torrents (files, URLs or magnet links) opened while running: straight
 * away if connected, otherwise once connected, like the command line
 * arguments. Takes ownership of uris. */
void trg_main_window_open(TrgMainWindow *win, gchar **uris)
{
    if (trg_client_is_connected(win->client)) {
        trg_add_from_filename(win, uris);
    } else if (win->args) {
        gchar **args = g_new0(gchar *, g_strv_length(win->args) + g_strv_length(uris) + 1);
        guint i = 0, j;

        for (j = 0; win->args[j]; j++)
            args[i++] = win->args[j];
        for (j = 0; uris[j]; j++)
            args[i++] = uris[j];

        g_free(win->args);
        g_free(uris);
        win->args = args;
    } else {
        win->args = uris;
    }

    gtk_window_present(GTK_WINDOW(win));
}

GMenuModel *trg_main_window_get_menu_model(TrgMainWindow *win)
{
    return trg_menu_bar_get_model(win->menuBar);
}

void auto_connect_if_required(TrgMainWindow *win)
{
    TrgPrefs *prefs = trg_client_get_prefs(win->client);
    gchar *host = trg_prefs_get_string(prefs, TRG_PREFS_KEY_HOSTNAME, TRG_PREFS_PROFILE);

    if (host) {
        gint len = strlen(host);
        g_free(host);
        if (len > 0 && trg_prefs_get_bool(prefs, TRG_PREFS_KEY_AUTO_CONNECT, TRG_PREFS_PROFILE)) {
            connect_cb(NULL, NULL, win);
        }
    }
}

TrgMainWindow *trg_main_window_new(TrgClient *tc)
{
    return g_object_new(TRG_TYPE_MAIN_WINDOW, "trg-client", tc, NULL);
}
