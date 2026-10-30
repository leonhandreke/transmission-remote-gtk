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
#include "trg-item.h"
#include "trg-main-window.h"
#include "trg-prefs.h"
#include "trg-trackers-model.h"
#include "trg-trackers-tree-view.h"
#include "trg-tree-view.h"
#include "util.h"

struct _TrgTrackersTreeView {
    TrgTreeView parent;

    TrgClient *client;
    trg_column_description *announceColumn;
    TrgTrackersModel *model;
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

    trg_trackers_model_set_accept(tv->model, TRUE);

    response->cb_data = tv->win;
    return on_generic_interactive_action_response(data);
}

void trg_trackers_tree_view_new_connection(TrgTrackersTreeView *tv, TrgClient *tc)
{
    trg_tree_view_set_column_editable(tv->announceColumn, is_tracker_edit_supported(tc));
}

/* Editing an announce URL replaces the tracker, or adds it for a new row */
static void trg_tracker_announce_edit(TrgTreeView *ttv, gpointer item, gboolean done,
                                      const gchar *new_text, gpointer data)
{
    TrgTrackersTreeView *tv = TRG_TRACKERS_TREE_VIEW(ttv);
    gint64 torrentId = trg_trackers_model_get_torrent_id(tv->model);
    JsonArray *torrentIds, *trackerModifiers;
    JsonNode *req;
    JsonObject *args;

    if (!done) {
        trg_trackers_model_set_accept(tv->model, FALSE);
        return;
    }

    if (!new_text || !*new_text) {
        trg_trackers_model_set_accept(tv->model, TRUE);
        return;
    }

    trg_item_set(item, TRACKERCOL_ANNOUNCE, new_text, -1);

    torrentIds = json_array_new();
    trackerModifiers = json_array_new();
    json_array_add_int_element(torrentIds, torrentId);
    req = torrent_set(torrentIds);
    args = node_get_arguments(req);

    if (!g_strcmp0(trg_item_get_string(item, TRACKERCOL_ICON), "list-add")) {
        json_array_add_string_element(trackerModifiers, new_text);
        json_object_set_array_member(args, "trackerAdd", trackerModifiers);
    } else {
        json_array_add_int_element(trackerModifiers, trg_item_get_int64(item, TRACKERCOL_ID));
        json_array_add_string_element(trackerModifiers, new_text);
        json_object_set_array_member(args, "trackerReplace", trackerModifiers);
    }

    dispatch_rpc_async(tv->client, req, on_trackers_update, tv);
}

static void trg_trackers_tree_view_init(TrgTrackersTreeView *self)
{
    TrgTreeView *ttv = TRG_TREE_VIEW(self);
    trg_column_description *desc;

    desc = trg_tree_view_reg_column(ttv, TRG_COLTYPE_ICONTEXT, TRACKERCOL_TIER, _("Tier"), "tier",
                                    TRG_COLUMN_UNREMOVABLE);
    desc->model_column_extra = TRACKERCOL_ICON;

    desc = trg_tree_view_reg_column(ttv, TRG_COLTYPE_TEXT, TRACKERCOL_ANNOUNCE, _("Announce URL"),
                                    "announce-url", TRG_COLUMN_UNREMOVABLE | TRG_COLUMN_EDITABLE);
    desc->edit = trg_tracker_announce_edit;
    desc->editable = FALSE;
    self->announceColumn = desc;

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

static void add_tracker(TrgTrackersTreeView *self)
{
    gpointer item = trg_trackers_model_add_new(self->model);

    trg_tree_view_select_item(TRG_TREE_VIEW(self), item);
    trg_tree_view_start_editing(TRG_TREE_VIEW(self), item, self->announceColumn);
}

static void delete_tracker(TrgTrackersTreeView *self)
{
    GList *items = trg_tree_view_get_selected_items(TRG_TREE_VIEW(self));
    GListStore *store = trg_list_model_get_store(TRG_LIST_MODEL(self->model));
    JsonArray *trackerIds = json_array_new();
    gint64 torrentId = trg_trackers_model_get_torrent_id(self->model);
    JsonArray *torrentIds = json_array_new();
    JsonNode *req;
    JsonObject *args;
    GList *li;

    for (li = items; li; li = g_list_next(li)) {
        guint position;

        json_array_add_int_element(trackerIds, trg_item_get_int64(li->data, TRACKERCOL_ID));
        if (g_list_store_find(store, li->data, &position))
            g_list_store_remove(store, position);
    }

    g_list_free(items);

    json_array_add_int_element(torrentIds, torrentId);
    req = torrent_set(torrentIds);
    args = node_get_arguments(req);

    json_object_set_array_member(args, "trackerRemove", trackerIds);

    trg_trackers_model_set_accept(self->model, FALSE);

    dispatch_rpc_async(self->client, req, on_trackers_update, self);
}

static void add_tracker_action_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    add_tracker(TRG_TRACKERS_TREE_VIEW(data));
}

static void delete_tracker_action_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    delete_tracker(TRG_TRACKERS_TREE_VIEW(data));
}

static void view_popup_menu(GtkWidget *treeview, gdouble x, gdouble y, gboolean with_delete)
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

static gboolean view_onContextMenu(GtkWidget *treeview, gdouble x, gdouble y, gpointer userdata)
{
    TrgTrackersTreeView *self = TRG_TRACKERS_TREE_VIEW(treeview);
    TrgTreeView *tv = TRG_TREE_VIEW(treeview);

    if (!is_tracker_edit_supported(self->client))
        return FALSE;

    /* from the keyboard */
    if (x < 0) {
        view_popup_menu(treeview, x, y, trg_tree_view_count_selected(tv) > 0);
        return TRUE;
    }

    /* the column headers have their own menu */
    if (trg_tree_view_point_in_header(tv, x, y))
        return FALSE;

    if (trg_tree_view_select_at(tv, x, y))
        view_popup_menu(treeview, x, y, TRUE);
    else if (trg_trackers_model_get_torrent_id(self->model) >= 0)
        view_popup_menu(treeview, x, y, FALSE);

    return TRUE;
}

TrgTrackersTreeView *trg_trackers_tree_view_new(TrgTrackersModel *model, TrgClient *client,
                                                TrgMainWindow *win, const gchar *configId)
{
    GObject *obj = g_object_new(TRG_TYPE_TRACKERS_TREE_VIEW, "config-id", configId, "prefs",
                                trg_client_get_prefs(client), NULL);
    TrgTrackersTreeView *self = TRG_TRACKERS_TREE_VIEW(obj);

    self->model = model;
    self->client = client;
    self->win = win;

    trg_tree_view_set_model(TRG_TREE_VIEW(obj), G_LIST_MODEL(model));
    trg_tree_view_setup_columns(TRG_TREE_VIEW(obj));
    trg_tree_view_restore_sort(TRG_TREE_VIEW(obj));

    trg_widget_add_context_menu(GTK_WIDGET(obj), view_onContextMenu, NULL);

    return TRG_TRACKERS_TREE_VIEW(obj);
}
