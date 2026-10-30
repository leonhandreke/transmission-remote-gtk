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
#include <json-glib/json-glib.h>

#include "protocol-constants.h"
#include "torrent.h"
#include "trg-client.h"
#include "trg-item.h"
#include "trg-model.h"
#include "trg-peers-model.h"
#include "trg-tree-view.h"

/* The peers of the selected torrent, updated by IP. The host names are
 * looked up asynchronously if the column is showing. */

static const TrgItemColumn peer_item_columns[] = {
    { "icon", G_TYPE_STRING },    { "ip", G_TYPE_STRING },       { "host", G_TYPE_STRING },
    { "flags", G_TYPE_STRING },   { "progress", G_TYPE_DOUBLE }, { "down-speed", G_TYPE_INT64 },
    { "up-speed", G_TYPE_INT64 }, { "client", G_TYPE_STRING },   { "update-serial", G_TYPE_INT64 },
};

#define TRG_TYPE_PEER_ITEM trg_peer_item_get_type()
TRG_DEFINE_ITEM_TYPE(TrgPeerItem, trg_peer_item, TRG, PEER_ITEM, peer_item_columns)

struct _TrgPeersModel {
    TrgListModel parent;

    /* IP -> item */
    GHashTable *peers;
};

G_DEFINE_TYPE(TrgPeersModel, trg_peers_model, TRG_TYPE_LIST_MODEL)

static void trg_peers_model_dispose(GObject *object)
{
    g_clear_pointer(&TRG_PEERS_MODEL(object)->peers, g_hash_table_destroy);

    G_OBJECT_CLASS(trg_peers_model_parent_class)->dispose(object);
}

static void trg_peers_model_class_init(TrgPeersModelClass *klass)
{
    G_OBJECT_CLASS(klass)->dispose = trg_peers_model_dispose;
}

static void trg_peers_model_init(TrgPeersModel *self)
{
    self->peers = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, g_object_unref);
}

static void resolved_dns_cb(GObject *source_object, GAsyncResult *res, gpointer data)
{
    GWeakRef *ref = data;
    TrgItem *item = g_weak_ref_get(ref);
    gchar *rdns = g_resolver_lookup_by_address_finish(G_RESOLVER(source_object), res, NULL);

    if (rdns && item)
        trg_item_set(item, PEERSCOL_HOST, rdns, -1);

    g_clear_object(&item);
    g_free(rdns);
    g_weak_ref_clear(ref);
    g_free(ref);
}

void trg_peers_model_clear(TrgPeersModel *model)
{
    g_hash_table_remove_all(model->peers);
    g_list_store_remove_all(trg_list_model_get_store(TRG_LIST_MODEL(model)));
}

void trg_peers_model_update(TrgPeersModel *model, TrgTreeView *tv, gint64 updateSerial,
                            JsonObject *t, gint mode)
{
    GListStore *store = trg_list_model_get_store(TRG_LIST_MODEL(model));
    gboolean doHostLookup = trg_tree_view_is_column_showing(tv, PEERSCOL_HOST);
    GPtrArray *added = g_ptr_array_new_with_free_func(g_object_unref);
    GList *li, *peersList;

    if (mode == TORRENT_GET_MODE_FIRST)
        trg_peers_model_clear(model);

    peersList = json_array_get_elements(torrent_get_peers(t));
    for (li = peersList; li; li = g_list_next(li)) {
        JsonObject *peer = json_node_get_object((JsonNode *)li->data);
        const gchar *address = peer_get_address(peer);
        TrgItem *item = g_hash_table_lookup(model->peers, address);

        if (!item) {
            item = g_object_new(TRG_TYPE_PEER_ITEM, NULL);
            trg_item_set(item, PEERSCOL_ICON, "network-workgroup", PEERSCOL_IP, address,
                         PEERSCOL_CLIENT, peer_get_client_name(peer), -1);
            g_hash_table_insert(model->peers, g_strdup(address), g_object_ref(item));
            g_ptr_array_add(added, item);

            if (doHostLookup) {
                GInetAddress *inetAddr = g_inet_address_new_from_string(address);
                GResolver *resolver = g_resolver_get_default();
                GWeakRef *ref = g_new0(GWeakRef, 1);

                g_weak_ref_init(ref, item);
                if (inetAddr)
                    g_resolver_lookup_by_address_async(resolver, inetAddr, NULL, resolved_dns_cb,
                                                       ref);
                g_object_unref(resolver);
                g_clear_object(&inetAddr);
            }
        }

        trg_item_set(item, PEERSCOL_FLAGS, peer_get_flagstr(peer), PEERSCOL_PROGRESS,
                     peer_get_progress(peer), PEERSCOL_DOWNSPEED, peer_get_rate_to_client(peer),
                     PEERSCOL_UPSPEED, peer_get_rate_to_peer(peer), PEERSCOL_UPDATESERIAL,
                     updateSerial, -1);
    }

    g_list_free(peersList);

    if (added->len > 0)
        g_list_store_splice(store, g_list_model_get_n_items(G_LIST_MODEL(store)), 0, added->pdata,
                            added->len);
    g_ptr_array_unref(added);

    if (mode != TORRENT_GET_MODE_FIRST) {
        GListModel *list = G_LIST_MODEL(store);
        guint i, n = g_list_model_get_n_items(list);

        for (i = 0; i < n; i++) {
            g_autoptr(TrgItem) item = g_list_model_get_item(list, i);
            if (trg_item_get_int64(item, PEERSCOL_UPDATESERIAL) != updateSerial)
                g_hash_table_remove(model->peers, trg_item_get_string(item, PEERSCOL_IP));
        }

        trg_model_remove_removed(store, PEERSCOL_UPDATESERIAL, updateSerial);
    }
}

TrgPeersModel *trg_peers_model_new(void)
{
    return g_object_new(TRG_TYPE_PEERS_MODEL, "item-type", TRG_TYPE_PEER_ITEM, NULL);
}
