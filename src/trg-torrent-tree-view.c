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

#include "torrent-row.h"
#include "trg-prefs.h"
#include "trg-torrent-model.h"
#include "trg-torrent-tree-view.h"
#include "trg-tree-view.h"

/* The list of torrents: the columns of a TrgTreeView in the classic style, a
 * list of TorrentRows in the Transmission styles. */

struct _TrgTorrentTreeView {
    TrgTreeView parent;

    TrgClient *client;
};

G_DEFINE_TYPE(TrgTorrentTreeView, trg_torrent_tree_view, TRG_TYPE_TREE_VIEW)

static void trg_torrent_tree_view_class_init(TrgTorrentTreeViewClass *klass G_GNUC_UNUSED)
{
}

static void trg_torrent_tree_view_init(TrgTorrentTreeView *tttv)
{
    TrgTreeView *ttv = TRG_TREE_VIEW(tttv);
    trg_column_description *desc;

    desc = trg_tree_view_reg_column(ttv, TRG_COLTYPE_ICONTEXT, TORRENT_COLUMN_NAME, _("Name"),
                                    "name", 0);
    desc->model_column_extra = TORRENT_COLUMN_ICON;

    trg_tree_view_reg_column(ttv, TRG_COLTYPE_SIZE, TORRENT_COLUMN_SIZEWHENDONE, _("Size"), "size",
                             0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_PROG, TORRENT_COLUMN_PERCENTDONE, _("Done"), "done",
                             0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_TEXT, TORRENT_COLUMN_STATUS, _("Status"), "status",
                             0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_NUMGTZERO, TORRENT_COLUMN_SEEDS, _("Seeds"), "seeds",
                             0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_NUMGTZERO, TORRENT_COLUMN_PEERS_TO_US, _("Sending"),
                             "sending", TRG_COLUMN_EXTRA);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_NUMGTZERO, TORRENT_COLUMN_LEECHERS, _("Leechers"),
                             "leechers", 0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_NUMGTZERO, TORRENT_COLUMN_DOWNLOADS, _("Downloads"),
                             "downloads", TRG_COLUMN_EXTRA);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_NUMGTZERO, TORRENT_COLUMN_PEERS_FROM_US,
                             _("Receiving"), "connected-leechers", TRG_COLUMN_EXTRA);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_NUMGTZERO, TORRENT_COLUMN_PEERS_CONNECTED,
                             _("Connected"), "connected-peers", 0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_NUMGTZERO, TORRENT_COLUMN_FROMPEX, _("PEX Peers"),
                             "from-pex", TRG_COLUMN_EXTRA | TRG_COLUMN_HIDE_FROM_TOP_MENU);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_NUMGTZERO, TORRENT_COLUMN_FROMDHT, _("DHT Peers"),
                             "from-dht", TRG_COLUMN_EXTRA | TRG_COLUMN_HIDE_FROM_TOP_MENU);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_NUMGTZERO, TORRENT_COLUMN_FROMTRACKERS,
                             _("Tracker Peers"), "from-trackers",
                             TRG_COLUMN_EXTRA | TRG_COLUMN_HIDE_FROM_TOP_MENU);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_NUMGTZERO, TORRENT_COLUMN_FROMLTEP, _("LTEP Peers"),
                             "from-ltep", TRG_COLUMN_EXTRA | TRG_COLUMN_HIDE_FROM_TOP_MENU);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_NUMGTZERO, TORRENT_COLUMN_FROMRESUME,
                             _("Resumed Peers"), "from-resume",
                             TRG_COLUMN_EXTRA | TRG_COLUMN_HIDE_FROM_TOP_MENU);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_NUMGTZERO, TORRENT_COLUMN_FROMINCOMING,
                             _("Incoming Peers"), "from-incoming",
                             TRG_COLUMN_EXTRA | TRG_COLUMN_HIDE_FROM_TOP_MENU);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_TEXT, TORRENT_COLUMN_PEER_SOURCES,
                             _("Peers T/I/E/H/X/L/R"), "peer-sources",
                             TRG_COLUMN_EXTRA | TRG_COLUMN_HIDE_FROM_TOP_MENU);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_SPEED, TORRENT_COLUMN_DOWNSPEED, _("Down Speed"),
                             "down-speed", 0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_SPEED, TORRENT_COLUMN_UPSPEED, _("Up Speed"),
                             "up-speed", 0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_ETA, TORRENT_COLUMN_ETA, _("ETA"), "eta", 0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_SIZE, TORRENT_COLUMN_UPLOADED, _("Uploaded"),
                             "uploaded", 0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_SIZE, TORRENT_COLUMN_DOWNLOADED, _("Downloaded"),
                             "downloaded", 0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_RATIO, TORRENT_COLUMN_RATIO, _("Ratio"), "ratio", 0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_EPOCH, TORRENT_COLUMN_ADDED, _("Added"), "added", 0);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_TEXT, TORRENT_COLUMN_TRACKERHOST, _("First Tracker"),
                             "first-tracker", TRG_COLUMN_EXTRA);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_TEXT, TORRENT_COLUMN_DOWNLOADDIR, _("Location"),
                             "download-dir", TRG_COLUMN_EXTRA);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_TEXT, TORRENT_COLUMN_ID, _("ID"), "id",
                             TRG_COLUMN_EXTRA);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_PRIO, TORRENT_COLUMN_BANDWIDTH_PRIORITY,
                             _("Priority"), "priority", TRG_COLUMN_EXTRA);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_NUMGTEQZERO, TORRENT_COLUMN_QUEUE_POSITION,
                             _("Queue Position"), "queue-position", TRG_COLUMN_EXTRA);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_EPOCH, TORRENT_COLUMN_DONE_DATE, _("Completed"),
                             "done-date", TRG_COLUMN_EXTRA);
    trg_tree_view_reg_column(ttv, TRG_COLTYPE_EPOCH, TORRENT_COLUMN_LASTACTIVE, _("Last Active"),
                             "last-active", TRG_COLUMN_EXTRA);

    trg_tree_view_set_search_column(ttv, TORRENT_COLUMN_NAME);
}

/* The IDs of the selected torrents */
JsonArray *build_json_id_array(TrgTorrentTreeView *tv)
{
    GList *items = trg_tree_view_get_selected_items(TRG_TREE_VIEW(tv));
    JsonArray *ids = json_array_new();
    GList *li;

    for (li = items; li; li = g_list_next(li))
        json_array_add_int_element(ids, trg_torrent_item_get_id(li->data));

    g_list_free(items);

    return ids;
}

static void torrent_row_setup(GtkSignalListItemFactory *factory, GtkListItem *list_item,
                              TrgTorrentTreeView *tv)
{
    gboolean compact = GPOINTER_TO_INT(g_object_get_data(G_OBJECT(factory), "compact"));
    GtkWidget *row = torrent_row_new(tv->client, compact);

    trg_tree_view_mark_row_widget(row, list_item);
    gtk_list_item_set_child(list_item, row);
}

static void torrent_row_bind(GtkSignalListItemFactory *factory, GtkListItem *list_item,
                             TrgTorrentTreeView *tv)
{
    torrent_row_set_item(TORRENT_ROW(gtk_list_item_get_child(list_item)),
                         gtk_list_item_get_item(list_item));
}

static void torrent_row_unbind(GtkSignalListItemFactory *factory, GtkListItem *list_item,
                               TrgTorrentTreeView *tv)
{
    torrent_row_set_item(TORRENT_ROW(gtk_list_item_get_child(list_item)), NULL);
}

/* The columns, or the rows of the (compact) Transmission style */
static void trg_torrent_tree_view_set_style(TrgTorrentTreeView *tv, gint64 style)
{
    GtkListItemFactory *factory;

    if (style == TRG_STYLE_CLASSIC) {
        trg_tree_view_set_list_factory(TRG_TREE_VIEW(tv), NULL);
        return;
    }

    factory = gtk_signal_list_item_factory_new();
    g_object_set_data(G_OBJECT(factory), "compact", GINT_TO_POINTER(style == TRG_STYLE_TR_COMPACT));
    g_signal_connect(factory, "setup", G_CALLBACK(torrent_row_setup), tv);
    g_signal_connect(factory, "bind", G_CALLBACK(torrent_row_bind), tv);
    g_signal_connect(factory, "unbind", G_CALLBACK(torrent_row_unbind), tv);

    trg_tree_view_set_list_factory(TRG_TREE_VIEW(tv), factory);
    g_object_unref(factory);
}

static void trg_torrent_tree_view_pref_changed(TrgPrefs *p, const gchar *updatedKey, gpointer data)
{
    if (!g_strcmp0(updatedKey, TRG_PREFS_KEY_STYLE))
        trg_torrent_tree_view_set_style(
            TRG_TORRENT_TREE_VIEW(data),
            trg_prefs_get_int(p, TRG_PREFS_KEY_STYLE, TRG_PREFS_GLOBAL));
}

/* model is a GListModel of TrgTorrentItems */
TrgTorrentTreeView *trg_torrent_tree_view_new(TrgClient *tc, GListModel *model)
{
    TrgPrefs *prefs = trg_client_get_prefs(tc);
    TrgTorrentTreeView *self = g_object_new(TRG_TYPE_TORRENT_TREE_VIEW, "prefs", prefs, NULL);
    TrgTreeView *tv = TRG_TREE_VIEW(self);

    self->client = tc;

    trg_tree_view_set_model(tv, model);
    trg_tree_view_setup_columns(tv);
    trg_tree_view_restore_sort(tv);

    trg_torrent_tree_view_set_style(
        self, trg_prefs_get_int(prefs, TRG_PREFS_KEY_STYLE, TRG_PREFS_GLOBAL));

    g_signal_connect_object(prefs, "pref-changed", G_CALLBACK(trg_torrent_tree_view_pref_changed),
                            self, 0);

    return self;
}
