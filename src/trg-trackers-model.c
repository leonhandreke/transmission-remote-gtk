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
#include <json-glib/json-glib.h>

#include "protocol-constants.h"
#include "torrent.h"
#include "trg-client.h"
#include "trg-item.h"
#include "trg-model.h"
#include "trg-trackers-model.h"

static const TrgItemColumn tracker_item_columns[] = {
    { "icon", G_TYPE_STRING },
    { "tier", G_TYPE_INT64 },
    { "announce", G_TYPE_STRING },
    { "scrape", G_TYPE_STRING },
    { "id", G_TYPE_INT64 },
    { "last-announce-peer-count", G_TYPE_INT64 },
    { "last-announce-time", G_TYPE_INT64 },
    { "last-scrape-time", G_TYPE_INT64 },
    { "seeder-count", G_TYPE_INT64 },
    { "leecher-count", G_TYPE_INT64 },
    { "host", G_TYPE_STRING },
    { "last-announce-result", G_TYPE_STRING },
    { "update-serial", G_TYPE_INT64 },
};

#define TRG_TYPE_TRACKER_ITEM trg_tracker_item_get_type()
TRG_DEFINE_ITEM_TYPE(TrgTrackerItem, trg_tracker_item, TRG, TRACKER_ITEM, tracker_item_columns)

struct _TrgTrackersModel {
    TrgListModel parent;

    gint64 torrentId;
    gint64 accept;
};

G_DEFINE_TYPE(TrgTrackersModel, trg_trackers_model, TRG_TYPE_LIST_MODEL)

void trg_trackers_model_set_no_selection(TrgTrackersModel *model)
{
    model->torrentId = -1;
}

gint64 trg_trackers_model_get_torrent_id(TrgTrackersModel *model)
{
    return model->torrentId;
}

void trg_trackers_model_clear(TrgTrackersModel *model)
{
    g_list_store_remove_all(trg_list_model_get_store(TRG_LIST_MODEL(model)));
}

/* A new row for a tracker to add, which the user edits the announce URL of */
gpointer trg_trackers_model_add_new(TrgTrackersModel *model)
{
    TrgItem *item = g_object_new(TRG_TYPE_TRACKER_ITEM, NULL);

    trg_item_set(item, TRACKERCOL_ICON, "list-add", TRACKERCOL_ID, (gint64)-1, -1);
    g_list_store_append(trg_list_model_get_store(TRG_LIST_MODEL(model)), item);
    g_object_unref(item);

    return item;
}

void trg_trackers_model_update(TrgTrackersModel *model, gint64 updateSerial, JsonObject *t,
                               gint mode)
{
    GListStore *store = trg_list_model_get_store(TRG_LIST_MODEL(model));
    JsonObject *tracker;
    gint64 trackerId;
    GList *trackers, *li;

    if (mode == TORRENT_GET_MODE_FIRST) {
        trg_trackers_model_clear(model);
        model->torrentId = torrent_get_id(t);
        model->accept = TRUE;
    } else if (!model->accept) {
        return;
    }

    trackers = json_array_get_elements(torrent_get_tracker_stats(t));

    for (li = trackers; li; li = g_list_next(li)) {
        TrgItem *item = NULL;

        tracker = json_node_get_object((JsonNode *)li->data);
        trackerId = tracker_stats_get_id(tracker);

        if (mode != TORRENT_GET_MODE_FIRST)
            item = trg_model_find_item(G_LIST_MODEL(store), TRACKERCOL_ID, trackerId, NULL);

        if (!item) {
            item = g_object_new(TRG_TYPE_TRACKER_ITEM, NULL);
            g_list_store_append(store, item);
            g_object_unref(item);
        }

        trg_item_set(
            item, TRACKERCOL_ICON, "network-workgroup", TRACKERCOL_ID, trackerId,
            TRACKERCOL_UPDATESERIAL, updateSerial, TRACKERCOL_TIER, tracker_stats_get_tier(tracker),
            TRACKERCOL_ANNOUNCE, tracker_stats_get_announce(tracker), TRACKERCOL_SCRAPE,
            tracker_stats_get_scrape(tracker), TRACKERCOL_HOST, tracker_stats_get_host(tracker),
            TRACKERCOL_LAST_ANNOUNCE_RESULT, tracker_stats_get_announce_result(tracker),
            TRACKERCOL_LAST_ANNOUNCE_TIME, tracker_stats_get_last_announce_time(tracker),
            TRACKERCOL_LAST_SCRAPE_TIME, tracker_stats_get_last_scrape_time(tracker),
            TRACKERCOL_LAST_ANNOUNCE_PEER_COUNT,
            tracker_stats_get_last_announce_peer_count(tracker), TRACKERCOL_LEECHERCOUNT,
            tracker_stats_get_leecher_count(tracker), TRACKERCOL_SEEDERCOUNT,
            tracker_stats_get_seeder_count(tracker), -1);
    }

    g_list_free(trackers);

    trg_model_remove_removed(store, TRACKERCOL_UPDATESERIAL, updateSerial);
}

static void trg_trackers_model_class_init(TrgTrackersModelClass *klass)
{
}

void trg_trackers_model_set_accept(TrgTrackersModel *model, gboolean accept)
{
    model->accept = accept;
}

static void trg_trackers_model_init(TrgTrackersModel *self)
{
    self->accept = TRUE;
    self->torrentId = -1;
}

TrgTrackersModel *trg_trackers_model_new(void)
{
    return g_object_new(TRG_TYPE_TRACKERS_MODEL, "item-type", TRG_TYPE_TRACKER_ITEM, NULL);
}
