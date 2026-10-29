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

#include <glib.h>
#include <glib/gi18n.h>
#include <gtk/gtk.h>

#include "json.h"
#include "requests.h"
#include "trg-client.h"
#include "trg-main-window.h"
#include "trg-prefs.h"
#include "trg-trackers-model.h"
#include "trg-trackers-tree-view.h"
#include "trg-tree-view.h"
#include "util.h"

struct _TrgTrackersTreeView {
    TrgTreeView parent;

    TrgClient *client;
    GtkCellRenderer *announceRenderer;
    GtkTreeViewColumn *announceColumn;
    TrgMainWindow *win;
};

G_DEFINE_TYPE(TrgTrackersTreeView, trg_trackers_tree_view, TRG_TYPE_TREE_VIEW)

static void trg_trackers_tree_view_class_init(TrgTrackersTreeViewClass *klass)
{
}

static gboolean is_tracker_edit_supported(TrgClient *tc)
{
    return trg_client_get_rpc_version(tc) >= 10;
}

static gboolean on_trackers_update(gpointer data)
{
    trg_response *response = (trg_response *)data;
    TrgTrackersTreeView *tv = TRG_TRACKERS_TREE_VIEW(response->cb_data);
    GtkTreeModel *model = gtk_tree_view_get_model(GTK_TREE_VIEW(response->cb_data));

    trg_trackers_model_set_accept(TRG_TRACKERS_MODEL(model), TRUE);

    response->cb_data = tv->win;
    return on_generic_interactive_action_response(data);
}

void trg_trackers_tree_view_new_connection(TrgTrackersTreeView *tv, TrgClient *tc)
{
    gboolean editable = is_tracker_edit_supported(tc);

    g_object_set(tv->announceRenderer, "editable", editable, "mode",
                 editable ? GTK_CELL_RENDERER_MODE_EDITABLE : GTK_CELL_RENDERER_MODE_INERT, NULL);
}

static void trg_tracker_announce_edited(GtkCellRendererText *renderer, gchar *path, gchar *new_text,
                                        gpointer user_data)
{
    TrgTrackersTreeView *tv = TRG_TRACKERS_TREE_VIEW(user_data);
    GtkTreeModel *model = gtk_tree_view_get_model(GTK_TREE_VIEW(user_data));
    gint64 torrentId = trg_trackers_model_get_torrent_id(TRG_TRACKERS_MODEL(model));
    JsonArray *torrentIds = json_array_new();
    JsonArray *trackerModifiers = json_array_new();

    gint64 trackerId;
    JsonNode *req;
    JsonObject *args;
    GtkTreeIter iter;
    gchar *icon;

    gtk_tree_model_get_iter_from_string(model, &iter, path);
    gtk_list_store_set(GTK_LIST_STORE(model), &iter, TRACKERCOL_ANNOUNCE, new_text, -1);
    gtk_tree_model_get(model, &iter, TRACKERCOL_ID, &trackerId, TRACKERCOL_ICON, &icon, -1);

    json_array_add_int_element(torrentIds, torrentId);

    req = torrent_set(torrentIds);
    args = node_get_arguments(req);

    if (!g_strcmp0(icon, "list-add")) {
        json_array_add_string_element(trackerModifiers, new_text);
        json_object_set_array_member(args, "trackerAdd", trackerModifiers);
    } else {
        json_array_add_int_element(trackerModifiers, trackerId);
        json_array_add_string_element(trackerModifiers, new_text);
        json_object_set_array_member(args, "trackerReplace", trackerModifiers);
    }

    g_free(icon);

    dispatch_rpc_async(tv->client, req, on_trackers_update, user_data);
}

static void trg_tracker_announce_editing_started(GtkCellRenderer *renderer G_GNUC_UNUSED,
                                                 GtkCellEditable *editable G_GNUC_UNUSED,
                                                 gchar *path G_GNUC_UNUSED, gpointer user_data)
{
    TrgTrackersModel *model = TRG_TRACKERS_MODEL(gtk_tree_view_get_model(GTK_TREE_VIEW(user_data)));

    trg_trackers_model_set_accept(model, FALSE);
}

static void trg_tracker_announce_editing_canceled(GtkWidget *w G_GNUC_UNUSED, gpointer data)
{
    TrgTrackersModel *model = TRG_TRACKERS_MODEL(gtk_tree_view_get_model(GTK_TREE_VIEW(data)));

    trg_trackers_model_set_accept(model, TRUE);
}

static void trg_trackers_tree_view_init(TrgTrackersTreeView *self)
{
    TrgTreeView *ttv = TRG_TREE_VIEW(self);
    trg_column_description *desc;

    desc = trg_tree_view_reg_column(ttv, TRG_COLTYPE_ICONTEXT, TRACKERCOL_TIER, _("Tier"), "tier",
                                    TRG_COLUMN_UNREMOVABLE);
    desc->model_column_extra = TRACKERCOL_ICON;

    desc = trg_tree_view_reg_column(ttv, TRG_COLTYPE_TEXT, TRACKERCOL_ANNOUNCE, _("Announce URL"),
                                    "announce-url", TRG_COLUMN_UNREMOVABLE);
    self->announceRenderer = desc->customRenderer = gtk_cell_renderer_text_new();
    g_signal_connect(self->announceRenderer, "edited", G_CALLBACK(trg_tracker_announce_edited),
                     self);
    g_signal_connect(self->announceRenderer, "editing-canceled",
                     G_CALLBACK(trg_tracker_announce_editing_canceled), self);
    g_signal_connect(self->announceRenderer, "editing-started",
                     G_CALLBACK(trg_tracker_announce_editing_started), self);
    desc->out = &self->announceColumn;

    trg_tree_view_reg_column(ttv, TRG_COLTYPE_NUMGTZERO, TRACKERCOL_LAST_ANNOUNCE_PEER_COUNT,
                             _("Peers"), "last-announce-peer-count", 0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_NUMGTZERO, TRACKERCOL_SEEDERCOUNT, _("Seeder Count"),
                             "seeder-count", 0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_NUMGTZERO, TRACKERCOL_LEECHERCOUNT,
                             _("Leecher Count"), "leecher-count", 0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_EPOCH, TRACKERCOL_LAST_ANNOUNCE_TIME,
                             _("Last Announce"), "last-announce-time", 0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_TEXT, TRACKERCOL_LAST_ANNOUNCE_RESULT,
                             _("Last Result"), "last-result", 0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_TEXT, TRACKERCOL_SCRAPE, _("Scrape URL"),
                             "scrape-url", 0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_EPOCH, TRACKERCOL_LAST_SCRAPE_TIME, _("Last Scrape"),
                             "last-scrape-time", TRG_COLUMN_EXTRA);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_TEXT, TRACKERCOL_HOST, _("Host"), "host",
                             TRG_COLUMN_EXTRA);
}

static void add_tracker(GtkWidget *w, gpointer data)
{
    GtkTreeView *tv = GTK_TREE_VIEW(data);
    TrgTrackersTreeView *self = TRG_TRACKERS_TREE_VIEW(data);
    GtkTreeModel *model = gtk_tree_view_get_model(tv);
    GtkTreeIter iter;
    GtkTreePath *path;

    gtk_list_store_append(GTK_LIST_STORE(model), &iter);
    gtk_list_store_set(GTK_LIST_STORE(model), &iter, TRACKERCOL_ICON, "list-add", -1);

    path = gtk_tree_model_get_path(model, &iter);
    gtk_tree_view_set_cursor(tv, path, self->announceColumn, TRUE);
    gtk_tree_path_free(path);
}

static void delete_tracker(GtkWidget *w, gpointer data)
{
    TrgTrackersTreeView *self = TRG_TRACKERS_TREE_VIEW(data);
    GtkTreeView *tv = GTK_TREE_VIEW(data);
    GList *selectionRefs = trg_tree_view_get_selected_refs_list(tv);
    GtkTreeModel *model = gtk_tree_view_get_model(tv);
    JsonArray *trackerIds = json_array_new();
    gint64 torrentId = trg_trackers_model_get_torrent_id(TRG_TRACKERS_MODEL(model));
    JsonArray *torrentIds = json_array_new();

    JsonNode *req;
    JsonObject *args;
    GList *li;

    for (li = selectionRefs; li; li = g_list_next(li)) {
        GtkTreeRowReference *rr = (GtkTreeRowReference *)li->data;
        GtkTreePath *path = gtk_tree_row_reference_get_path(rr);
        if (path) {
            gint64 trackerId;
            GtkTreeIter trackerIter;
            gtk_tree_model_get_iter(model, &trackerIter, path);
            gtk_tree_model_get(model, &trackerIter, TRACKERCOL_ID, &trackerId, -1);
            json_array_add_int_element(trackerIds, trackerId);
            gtk_list_store_remove(GTK_LIST_STORE(model), &trackerIter);
            gtk_tree_path_free(path);
        }
        gtk_tree_row_reference_free(rr);
    }
    g_list_free(selectionRefs);

    json_array_add_int_element(torrentIds, torrentId);

    req = torrent_set(torrentIds);
    args = node_get_arguments(req);

    json_object_set_array_member(args, "trackerRemove", trackerIds);

    trg_trackers_model_set_accept(TRG_TRACKERS_MODEL(model), FALSE);

    dispatch_rpc_async(self->client, req, on_trackers_update, data);
}

static void add_tracker_action_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    add_tracker(NULL, data);
}

static void delete_tracker_action_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    delete_tracker(NULL, data);
}

static void view_popup_menu_full(GtkWidget *treeview, gdouble x, gdouble y, gboolean with_delete)
{
    static const GActionEntry entries[] = {
        { "delete", delete_tracker_action_cb, NULL, NULL, NULL, { 0 } },
        { "add", add_tracker_action_cb, NULL, NULL, NULL, { 0 } },
    };
    GSimpleActionGroup *actions = g_simple_action_group_new();
    GMenu *menu = g_menu_new();

    g_action_map_add_action_entries(G_ACTION_MAP(actions), entries, G_N_ELEMENTS(entries),
                                    treeview);

    if (with_delete)
        g_menu_append(menu, _("Delete"), "trackers.delete");
    g_menu_append(menu, _("Add"), "trackers.add");

    trg_popover_menu_popup(treeview, G_MENU_MODEL(menu), x, y, "trackers", G_ACTION_GROUP(actions));

    g_object_unref(menu);
    g_object_unref(actions);
}

static void view_popup_menu_add_only(GtkWidget *treeview, gdouble x, gdouble y,
                                     gpointer data G_GNUC_UNUSED)
{
    view_popup_menu_full(treeview, x, y, FALSE);
}

static void view_popup_menu(GtkWidget *treeview, gdouble x, gdouble y, gpointer data G_GNUC_UNUSED)
{
    view_popup_menu_full(treeview, x, y, TRUE);
}

static gboolean view_onButtonPressed(GtkWidget *treeview, gdouble x, gdouble y, gpointer userdata)
{
    TrgTrackersTreeView *self = TRG_TRACKERS_TREE_VIEW(treeview);
    TrgTrackersModel *model = TRG_TRACKERS_MODEL(gtk_tree_view_get_model(GTK_TREE_VIEW(treeview)));
    GtkTreeSelection *selection;
    GtkTreePath *path;
    gint bx, by;

    if (!is_tracker_edit_supported(self->client))
        return FALSE;

    selection = gtk_tree_view_get_selection(GTK_TREE_VIEW(treeview));
    gtk_tree_view_convert_widget_to_bin_window_coords(GTK_TREE_VIEW(treeview), (gint)x, (gint)y,
                                                      &bx, &by);

    if (gtk_tree_view_get_path_at_pos(GTK_TREE_VIEW(treeview), bx, by, &path, NULL, NULL, NULL)) {
        if (!gtk_tree_selection_path_is_selected(selection, path)) {
            gtk_tree_selection_unselect_all(selection);
            gtk_tree_selection_select_path(selection, path);
        }
        gtk_tree_path_free(path);

        view_popup_menu(treeview, x, y, userdata);
    } else if (trg_trackers_model_get_torrent_id(model) >= 0) {
        view_popup_menu_add_only(treeview, x, y, userdata);
    }
    return TRUE;
}

static gboolean view_onContextMenu(GtkWidget *treeview, gdouble x, gdouble y, gpointer userdata)
{
    /* from the keyboard */
    if (x < 0) {
        view_popup_menu(treeview, x, y, userdata);
        return TRUE;
    }

    return view_onButtonPressed(treeview, x, y, userdata);
}

TrgTrackersTreeView *trg_trackers_tree_view_new(TrgTrackersModel *model, TrgClient *client,
                                                TrgMainWindow *win, const gchar *configId)
{
    GObject *obj = g_object_new(TRG_TYPE_TRACKERS_TREE_VIEW, "config-id", configId, "prefs",
                                trg_client_get_prefs(client), NULL);

    TrgTrackersTreeView *self = TRG_TRACKERS_TREE_VIEW(obj);

    gtk_tree_view_set_model(GTK_TREE_VIEW(obj), GTK_TREE_MODEL(model));
    self->client = client;
    self->win = win;

    trg_tree_view_setup_columns(TRG_TREE_VIEW(obj));

    trg_widget_add_context_menu(GTK_WIDGET(obj), view_onContextMenu, NULL);

    return TRG_TRACKERS_TREE_VIEW(obj);
}
