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
#include <json-glib/json-glib.h>
#include <string.h>

#include "json.h"
#include "protocol-constants.h"
#include "torrent.h"
#include "trg-item.h"
#include "trg-torrent-model.h"
#include "util.h"

/* A GListModel of TrgTorrentItems which updates from a JSON torrent-get
 * response. It handles a number of different update modes.
 *   1) The first update.
 *   2) A full update.
 *   3) An active-only update.
 *   4) Individual torrent updates.
 *
 * The items are updated in place (only the values that changed notify), and
 * looked up by ID in a hash table, so an update is linear in the number of
 * torrents.
 *
 * Other stuff it does.
 *   1) Populates a stats struct with speeds/state counts as it works through the
 *      response.
 *   2) Emits signals if something is added or removed. This is used by the state
 *      selector so it doesn't have to refresh itself on every update.
 *   3) Added or completed signals, for notifications.
 *   4) Maintains the torrent hash table (by ID).
 *      (and provide a lookup function which outputs an item and/or JSON object.)
 *   5) If the download directory is new/changed, create a short version if there
 *      is one (duplicate it not) for the state selector to filter/populate against.
 *   6) Shorten the tracker announce URL.
 */

static const TrgItemColumn torrent_item_columns[TORRENT_COLUMN_COLUMNS] = {
    [TORRENT_COLUMN_ICON] = { "icon", G_TYPE_STRING },
    [TORRENT_COLUMN_NAME] = { "name", G_TYPE_STRING },
    [TORRENT_COLUMN_SIZEWHENDONE] = { "size-when-done", G_TYPE_INT64 },
    [TORRENT_COLUMN_PERCENTDONE] = { "percent-done", G_TYPE_DOUBLE },
    [TORRENT_COLUMN_METADATAPERCENTCOMPLETE] = { "metadata-percent-complete", G_TYPE_DOUBLE },
    [TORRENT_COLUMN_STATUS] = { "status", G_TYPE_STRING },
    [TORRENT_COLUMN_SEEDS] = { "seeds", G_TYPE_INT64 },
    [TORRENT_COLUMN_LEECHERS] = { "leechers", G_TYPE_INT64 },
    [TORRENT_COLUMN_DOWNLOADS] = { "downloads", G_TYPE_INT64 },
    [TORRENT_COLUMN_PEERS_CONNECTED] = { "peers-connected", G_TYPE_INT64 },
    [TORRENT_COLUMN_PEERS_FROM_US] = { "peers-from-us", G_TYPE_INT64 },
    [TORRENT_COLUMN_WEB_SEEDS_TO_US] = { "web-seeds-to-us", G_TYPE_INT64 },
    [TORRENT_COLUMN_PEERS_TO_US] = { "peers-to-us", G_TYPE_INT64 },
    [TORRENT_COLUMN_DOWNSPEED] = { "down-speed", G_TYPE_INT64 },
    [TORRENT_COLUMN_UPSPEED] = { "up-speed", G_TYPE_INT64 },
    [TORRENT_COLUMN_ETA] = { "eta", G_TYPE_INT64 },
    [TORRENT_COLUMN_UPLOADED] = { "uploaded", G_TYPE_INT64 },
    [TORRENT_COLUMN_DOWNLOADED] = { "downloaded", G_TYPE_INT64 },
    [TORRENT_COLUMN_TOTALSIZE] = { "total-size", G_TYPE_INT64 },
    [TORRENT_COLUMN_HAVE_UNCHECKED] = { "have-unchecked", G_TYPE_INT64 },
    [TORRENT_COLUMN_HAVE_VALID] = { "have-valid", G_TYPE_INT64 },
    [TORRENT_COLUMN_RATIO] = { "ratio", G_TYPE_DOUBLE },
    [TORRENT_COLUMN_ADDED] = { "added", G_TYPE_INT64 },
    [TORRENT_COLUMN_ID] = { "id", G_TYPE_INT64 },
    [TORRENT_COLUMN_JSON] = { "json", G_TYPE_INVALID },
    [TORRENT_COLUMN_UPDATESERIAL] = { "update-serial", G_TYPE_INT64 },
    [TORRENT_COLUMN_FLAGS] = { "flags", G_TYPE_UINT },
    [TORRENT_COLUMN_DOWNLOADDIR] = { "download-dir", G_TYPE_STRING },
    [TORRENT_COLUMN_DOWNLOADDIR_SHORT] = { "download-dir-short", G_TYPE_STRING },
    [TORRENT_COLUMN_BANDWIDTH_PRIORITY] = { "bandwidth-priority", G_TYPE_INT64 },
    [TORRENT_COLUMN_DONE_DATE] = { "done-date", G_TYPE_INT64 },
    [TORRENT_COLUMN_FROMPEX] = { "from-pex", G_TYPE_INT64 },
    [TORRENT_COLUMN_FROMDHT] = { "from-dht", G_TYPE_INT64 },
    [TORRENT_COLUMN_FROMTRACKERS] = { "from-trackers", G_TYPE_INT64 },
    [TORRENT_COLUMN_FROMLTEP] = { "from-ltep", G_TYPE_INT64 },
    [TORRENT_COLUMN_FROMRESUME] = { "from-resume", G_TYPE_INT64 },
    [TORRENT_COLUMN_FROMINCOMING] = { "from-incoming", G_TYPE_INT64 },
    [TORRENT_COLUMN_PEER_SOURCES] = { "peer-sources", G_TYPE_STRING },
    [TORRENT_COLUMN_TRACKERHOST] = { "tracker-host", G_TYPE_STRING },
    [TORRENT_COLUMN_QUEUE_POSITION] = { "queue-position", G_TYPE_INT64 },
    [TORRENT_COLUMN_LASTACTIVE] = { "last-active", G_TYPE_INT64 },
    [TORRENT_COLUMN_FILECOUNT] = { "file-count", G_TYPE_UINT },
    [TORRENT_COLUMN_ERROR] = { "error", G_TYPE_INT64 },
    [TORRENT_COLUMN_SEED_RATIO_MODE] = { "seed-ratio-mode", G_TYPE_INT64 },
    [TORRENT_COLUMN_SEED_RATIO_LIMIT] = { "seed-ratio-limit", G_TYPE_DOUBLE },
};

struct _TrgTorrentItem {
    TrgItem parent;
};

G_DEFINE_TYPE(TrgTorrentItem, trg_torrent_item, TRG_TYPE_ITEM)

static void trg_torrent_item_class_init(TrgTorrentItemClass *klass)
{
    TrgItemColumn columns[TORRENT_COLUMN_COLUMNS];

    memcpy(columns, torrent_item_columns, sizeof(columns));
    columns[TORRENT_COLUMN_JSON].type = JSON_TYPE_OBJECT;

    trg_item_class_install_columns(klass, columns, TORRENT_COLUMN_COLUMNS);
}

static void trg_torrent_item_init(TrgTorrentItem *self)
{
}

JsonObject *trg_torrent_item_get_json(TrgTorrentItem *item)
{
    return trg_item_get_boxed(item, TORRENT_COLUMN_JSON);
}

gint64 trg_torrent_item_get_id(TrgTorrentItem *item)
{
    return trg_item_get_int64(item, TORRENT_COLUMN_ID);
}

enum {
    TMODEL_TORRENT_COMPLETED,
    TMODEL_UPDATE,
    TMODEL_TORRENT_ADDED,
    TMODEL_STATE_CHANGED,
    TMODEL_SIGNAL_COUNT
};

static guint signals[TMODEL_SIGNAL_COUNT] = { 0 };

struct _TrgTorrentModel {
    TrgListModel parent;

    GHashTable *ht;
    GRegex *urlHostRegex;
    trg_torrent_model_update_stats stats;
};

G_DEFINE_TYPE(TrgTorrentModel, trg_torrent_model, TRG_TYPE_LIST_MODEL)

static GListStore *trg_torrent_model_store(TrgTorrentModel *model)
{
    return trg_list_model_get_store(TRG_LIST_MODEL(model));
}

static void trg_torrent_model_dispose(GObject *object)
{
    TrgTorrentModel *self = TRG_TORRENT_MODEL(object);

    g_clear_pointer(&self->ht, g_hash_table_destroy);
    g_clear_pointer(&self->urlHostRegex, g_regex_unref);

    G_OBJECT_CLASS(trg_torrent_model_parent_class)->dispose(object);
}

static void trg_torrent_model_class_init(TrgTorrentModelClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);

    object_class->dispose = trg_torrent_model_dispose;

    signals[TMODEL_TORRENT_COMPLETED]
        = g_signal_new("torrent-completed", G_TYPE_FROM_CLASS(object_class), G_SIGNAL_RUN_LAST, 0,
                       NULL, NULL, NULL, G_TYPE_NONE, 1, TRG_TYPE_TORRENT_ITEM);

    signals[TMODEL_UPDATE] = g_signal_new("update", G_TYPE_FROM_CLASS(object_class),
                                          G_SIGNAL_RUN_LAST, 0, NULL, NULL, NULL, G_TYPE_NONE, 0);

    signals[TMODEL_TORRENT_ADDED]
        = g_signal_new("torrent-added", G_TYPE_FROM_CLASS(object_class), G_SIGNAL_RUN_LAST, 0, NULL,
                       NULL, NULL, G_TYPE_NONE, 1, TRG_TYPE_TORRENT_ITEM);

    signals[TMODEL_STATE_CHANGED]
        = g_signal_new("torrents-state-change", G_TYPE_FROM_CLASS(object_class), G_SIGNAL_RUN_LAST,
                       0, NULL, NULL, NULL, G_TYPE_NONE, 1, G_TYPE_UINT);
}

trg_torrent_model_update_stats *trg_torrent_model_get_stats(TrgTorrentModel *model)
{
    return &(model->stats);
}

static void trg_torrent_model_count_peers(TrgTorrentItem *item, JsonObject *t)
{
    GList *trackersList = json_array_get_elements(torrent_get_tracker_stats(t));
    gint64 seeders = 0;
    gint64 leechers = 0;
    gint64 downloads = 0;
    GList *li;

    for (li = trackersList; li; li = g_list_next(li)) {
        JsonObject *tracker = json_node_get_object((JsonNode *)li->data);

        seeders = MAX(seeders, tracker_stats_get_seeder_count(tracker));
        leechers = MAX(leechers, tracker_stats_get_leecher_count(tracker));
        downloads += tracker_stats_get_download_count(tracker);
    }

    g_list_free(trackersList);

    trg_item_set(item, TORRENT_COLUMN_SEEDS, seeders, TORRENT_COLUMN_LEECHERS, leechers,
                 TORRENT_COLUMN_DOWNLOADS, downloads, -1);
}

static void trg_torrent_model_init(TrgTorrentModel *self)
{
    self->ht = g_hash_table_new_full(g_int64_hash, g_int64_equal, g_free, g_object_unref);

    self->urlHostRegex = trg_uri_host_regex_new();
}

void trg_torrent_model_reload_dir_aliases(TrgClient *tc, TrgTorrentModel *model)
{
    GListModel *list = G_LIST_MODEL(model);
    guint i, n = g_list_model_get_n_items(list);

    for (i = 0; i < n; i++) {
        g_autoptr(TrgItem) item = g_list_model_get_item(list, i);
        gchar *shortDownloadDir
            = shorten_download_dir(tc, trg_item_get_string(item, TORRENT_COLUMN_DOWNLOADDIR));

        trg_item_set(item, TORRENT_COLUMN_DOWNLOADDIR_SHORT, shortDownloadDir, -1);
        g_free(shortDownloadDir);
    }

    g_signal_emit(model, signals[TMODEL_STATE_CHANGED], 0, TORRENT_UPDATE_PATH_CHANGE);
}

static void trg_torrent_model_stats_scan(TrgTorrentModel *model)
{
    trg_torrent_model_update_stats *stats = &model->stats;
    GListModel *list = G_LIST_MODEL(model);
    guint i, n = g_list_model_get_n_items(list);

    stats->count = stats->down = stats->error = stats->paused = stats->seeding = stats->complete
        = stats->incomplete = stats->active = stats->checking = stats->seed_wait = stats->down_wait
        = 0;

    for (i = 0; i < n; i++) {
        g_autoptr(TrgItem) item = g_list_model_get_item(list, i);
        guint flags = trg_item_get_uint(item, TORRENT_COLUMN_FLAGS);

        if (flags & TORRENT_FLAG_SEEDING)
            stats->seeding++;
        else if (flags & TORRENT_FLAG_DOWNLOADING)
            stats->down++;
        else if (flags & TORRENT_FLAG_PAUSED)
            stats->paused++;

        if (flags & TORRENT_FLAG_ERROR)
            stats->error++;

        if (flags & TORRENT_FLAG_COMPLETE)
            stats->complete++;
        else
            stats->incomplete++;

        if (flags & TORRENT_FLAG_CHECKING_ANY)
            stats->checking++;

        if (flags & TORRENT_FLAG_ACTIVE)
            stats->active++;

        if (flags & TORRENT_FLAG_SEEDING_WAIT)
            stats->seed_wait++;

        if (flags & TORRENT_FLAG_DOWNLOADING_WAIT)
            stats->down_wait++;

        stats->count++;
    }
}

void trg_torrent_model_remove_all(TrgTorrentModel *model)
{
    g_hash_table_remove_all(model->ht);
    g_list_store_remove_all(trg_torrent_model_store(model));
}

gchar *shorten_download_dir(TrgClient *tc, const gchar *downloadDir)
{
    TrgPrefs *prefs = trg_client_get_prefs(tc);
    JsonArray *labels
        = trg_prefs_get_array(prefs, TRG_PREFS_KEY_DESTINATIONS, TRG_PREFS_CONNECTION);
    JsonObject *session = trg_client_get_session(tc);
    const gchar *defaultDownloadDir = session_get_download_dir(session);
    gchar *shortDownloadDir = NULL;

    if (labels) {
        GList *labelsList = json_array_get_elements(labels);
        if (labelsList) {
            GList *li;
            for (li = labelsList; li; li = g_list_next(li)) {
                JsonObject *labelObj = json_node_get_object((JsonNode *)li->data);
                const gchar *labelDir = json_object_get_string_member(
                    labelObj, TRG_PREFS_KEY_DESTINATIONS_SUBKEY_DIR);
                if (!g_strcmp0(downloadDir, labelDir)) {
                    const gchar *labelLabel
                        = json_object_get_string_member(labelObj, TRG_PREFS_SUBKEY_LABEL);
                    shortDownloadDir = g_strdup(labelLabel);
                    break;
                }
            }
            g_list_free(labelsList);
        }
    }

    if (shortDownloadDir) {
        return shortDownloadDir;
    } else {
        if (!g_strcmp0(defaultDownloadDir, downloadDir))
            return g_strdup(_("Default"));

        if (g_str_has_prefix(downloadDir, defaultDownloadDir)) {
            size_t offset = strlen(defaultDownloadDir);
            if (*(downloadDir + offset) == '/')
                offset++;

            if (offset < strlen(downloadDir))
                return g_strdup(downloadDir + offset);
        }
    }

    return g_strdup(downloadDir);
}

static void update_torrent_item(TrgTorrentModel *model, TrgClient *tc, gint64 rpcv, gint64 serial,
                                TrgTorrentItem *item, JsonObject *t,
                                trg_torrent_model_update_stats *stats, guint *whatsChanged)
{
    guint lastFlags, newFlags;
    JsonObject *pf;
    JsonArray *trackerStats;
    gchar *statusString, *statusIcon, *downloadDir;
    gint64 downRate, upRate, haveValid, uploaded, downloaded, id, status, lpd;
    guint fileCount;
    gchar *firstTrackerHost = NULL;
    gchar *peerSources = NULL;
    gchar *lastDownloadDir;

    downRate = torrent_get_rate_down(t);
    stats->downRateTotal += downRate;

    upRate = torrent_get_rate_up(t);
    stats->upRateTotal += upRate;

    uploaded = torrent_get_uploaded(t);
    downloaded = torrent_get_downloaded(t);
    haveValid = torrent_get_have_valid(t);

    downloadDir = (gchar *)torrent_get_download_dir(t);
    rm_trailing_slashes(downloadDir);

    id = torrent_get_id(t);
    status = torrent_get_status(t);
    fileCount = json_array_get_length(torrent_get_files(t));
    newFlags = torrent_get_flags(t, rpcv, status, fileCount, downRate, upRate);
    statusString = torrent_get_status_string(rpcv, status, newFlags);
    statusIcon = torrent_get_status_icon(rpcv, newFlags);
    pf = torrent_get_peersfrom(t);
    trackerStats = torrent_get_tracker_stats(t);

    lastFlags = trg_item_get_uint(item, TORRENT_COLUMN_FLAGS);
    lastDownloadDir = g_strdup(trg_item_get_string(item, TORRENT_COLUMN_DOWNLOADDIR));

    if (json_array_get_length(trackerStats) > 0) {
        JsonObject *firstTracker = json_array_get_object_element(trackerStats, 0);
        firstTrackerHost
            = trg_gregex_get_first(model->urlHostRegex, tracker_stats_get_host(firstTracker));
    }

    lpd = peerfrom_get_lpd(pf);
    if (newFlags & TORRENT_FLAG_ACTIVE) {
        if (lpd >= 0) {
            peerSources = g_strdup_printf(
                "%" G_GINT64_FORMAT " / %" G_GINT64_FORMAT " / %" G_GINT64_FORMAT
                " / %" G_GINT64_FORMAT " / %" G_GINT64_FORMAT " / %" G_GINT64_FORMAT
                " / %" G_GINT64_FORMAT,
                peerfrom_get_trackers(pf), peerfrom_get_incoming(pf), peerfrom_get_ltep(pf),
                peerfrom_get_dht(pf), peerfrom_get_pex(pf), lpd, peerfrom_get_resume(pf));
        } else {
            peerSources = g_strdup_printf(
                "%" G_GINT64_FORMAT " / %" G_GINT64_FORMAT " / %" G_GINT64_FORMAT
                " / %" G_GINT64_FORMAT " / %" G_GINT64_FORMAT " / N/A / %" G_GINT64_FORMAT,
                peerfrom_get_trackers(pf), peerfrom_get_incoming(pf), peerfrom_get_ltep(pf),
                peerfrom_get_dht(pf), peerfrom_get_pex(pf), peerfrom_get_resume(pf));
        }
    }

    /* Only the values which changed notify, all together when it's done. The
     * update serial is set last, so a cell watching it sees all the changes. */
    g_object_freeze_notify(G_OBJECT(item));

    trg_item_set(
        item, TORRENT_COLUMN_ICON, statusIcon, TORRENT_COLUMN_ADDED, torrent_get_added_date(t),
        TORRENT_COLUMN_FILECOUNT, fileCount, TORRENT_COLUMN_DONE_DATE, torrent_get_done_date(t),
        TORRENT_COLUMN_NAME, torrent_get_name(t), TORRENT_COLUMN_ERROR, torrent_get_error(t),
        TORRENT_COLUMN_SIZEWHENDONE, torrent_get_size_when_done(t), TORRENT_COLUMN_PERCENTDONE,
        (newFlags & TORRENT_FLAG_CHECKING) ? torrent_get_recheck_progress(t)
                                           : torrent_get_percent_done(t),
        TORRENT_COLUMN_METADATAPERCENTCOMPLETE, torrent_get_metadata_percent_complete(t),
        TORRENT_COLUMN_STATUS, statusString, TORRENT_COLUMN_DOWNSPEED, downRate,
        TORRENT_COLUMN_FLAGS, newFlags, TORRENT_COLUMN_UPSPEED, upRate, TORRENT_COLUMN_ETA,
        torrent_get_eta(t), TORRENT_COLUMN_UPLOADED, uploaded, TORRENT_COLUMN_DOWNLOADED,
        downloaded, TORRENT_COLUMN_TOTALSIZE, torrent_get_total_size(t),
        TORRENT_COLUMN_HAVE_UNCHECKED, torrent_get_have_unchecked(t), TORRENT_COLUMN_HAVE_VALID,
        haveValid, TORRENT_COLUMN_FROMPEX, peerfrom_get_pex(pf), TORRENT_COLUMN_FROMDHT,
        peerfrom_get_dht(pf), TORRENT_COLUMN_FROMTRACKERS, peerfrom_get_trackers(pf),
        TORRENT_COLUMN_FROMLTEP, peerfrom_get_ltep(pf), TORRENT_COLUMN_FROMRESUME,
        peerfrom_get_resume(pf), TORRENT_COLUMN_FROMINCOMING, peerfrom_get_incoming(pf),
        TORRENT_COLUMN_PEER_SOURCES, peerSources, TORRENT_COLUMN_PEERS_CONNECTED,
        torrent_get_peers_connected(t), TORRENT_COLUMN_PEERS_TO_US,
        torrent_get_peers_sending_to_us(t), TORRENT_COLUMN_PEERS_FROM_US,
        torrent_get_peers_getting_from_us(t), TORRENT_COLUMN_WEB_SEEDS_TO_US,
        torrent_get_web_seeds_sending_to_us(t), TORRENT_COLUMN_QUEUE_POSITION,
        torrent_get_queue_position(t), TORRENT_COLUMN_SEED_RATIO_LIMIT,
        torrent_get_seed_ratio_limit(t), TORRENT_COLUMN_SEED_RATIO_MODE,
        torrent_get_seed_ratio_mode(t), TORRENT_COLUMN_LASTACTIVE, torrent_get_activity_date(t),
        TORRENT_COLUMN_RATIO,
        uploaded > 0 && haveValid > 0 ? (double)uploaded / (double)haveValid : 0,
        TORRENT_COLUMN_DOWNLOADDIR, downloadDir, TORRENT_COLUMN_BANDWIDTH_PRIORITY,
        torrent_get_bandwidth_priority(t), TORRENT_COLUMN_ID, id, TORRENT_COLUMN_JSON, t,
        TORRENT_COLUMN_TRACKERHOST, firstTrackerHost ? firstTrackerHost : "", -1);

    if (!lastDownloadDir || g_strcmp0(downloadDir, lastDownloadDir)) {
        gchar *shortDownloadDir = shorten_download_dir(tc, downloadDir);
        trg_item_set(item, TORRENT_COLUMN_DOWNLOADDIR_SHORT, shortDownloadDir, -1);
        g_free(shortDownloadDir);
        *whatsChanged |= TORRENT_UPDATE_PATH_CHANGE;
    }

    trg_torrent_model_count_peers(item, t);

    trg_item_set(item, TORRENT_COLUMN_UPDATESERIAL, serial, -1);

    g_object_thaw_notify(G_OBJECT(item));

    if ((lastFlags & TORRENT_FLAG_DOWNLOADING) && (!(newFlags & TORRENT_FLAG_DOWNLOADING))
        && (newFlags & TORRENT_FLAG_COMPLETE))
        g_signal_emit(model, signals[TMODEL_TORRENT_COMPLETED], 0, item);

    if (lastFlags != newFlags)
        *whatsChanged |= TORRENT_UPDATE_STATE_CHANGE;

    g_free(firstTrackerHost);
    g_free(peerSources);
    g_free(lastDownloadDir);
    g_free(statusString);
    g_free(statusIcon);
}

TrgTorrentModel *trg_torrent_model_new(void)
{
    return g_object_new(TRG_TYPE_TORRENT_MODEL, "item-type", TRG_TYPE_TORRENT_ITEM, NULL);
}

GHashTable *get_torrent_table(TrgTorrentModel *model)
{
    return model->ht;
}

/* The download directories of all the torrents, newly allocated */
GList *trg_torrent_model_get_download_dirs(GHashTable *table)
{
    GList *values = g_hash_table_get_values(table);
    GList *dirs = NULL;
    GList *li;

    for (li = values; li; li = g_list_next(li)) {
        const gchar *dd = trg_item_get_string(li->data, TORRENT_COLUMN_DOWNLOADDIR);
        if (dd)
            dirs = g_list_prepend(dirs, g_strdup(dd));
    }

    g_list_free(values);

    return dirs;
}

/* Looks up a torrent by ID, giving its JSON object and/or item (not new
 * references). */
gboolean get_torrent_data(GHashTable *table, gint64 id, JsonObject **t, TrgTorrentItem **out_item)
{
    TrgTorrentItem *item = g_hash_table_lookup(table, &id);

    if (!item)
        return FALSE;

    if (out_item)
        *out_item = item;
    if (t)
        *t = trg_torrent_item_get_json(item);

    return TRUE;
}

/* Removes the stale torrents: those marked with the update serial -1, and if
 * serial isn't -1, those which weren't updated with it. They're removed in
 * runs, from the end, so this is linear in the number of torrents. Returns
 * whether any were removed. */
static gboolean trg_torrent_model_remove_stale(TrgTorrentModel *model, gint64 serial)
{
    GListStore *store = trg_torrent_model_store(model);
    GListModel *list = G_LIST_MODEL(store);
    guint i = g_list_model_get_n_items(list);
    gboolean removed = FALSE;

    while (i > 0) {
        guint end = i;

        while (i > 0) {
            g_autoptr(TrgItem) item = g_list_model_get_item(list, i - 1);
            gint64 itemSerial = trg_item_get_int64(item, TORRENT_COLUMN_UPDATESERIAL);
            gint64 id;

            if (itemSerial != -1 && (serial == -1 || itemSerial == serial))
                break;

            id = trg_item_get_int64(item, TORRENT_COLUMN_ID);
            g_hash_table_remove(model->ht, &id);
            i--;
        }

        if (i < end) {
            g_list_store_splice(store, i, end - i, NULL, 0);
            removed = TRUE;
        } else {
            i--;
        }
    }

    return removed;
}

trg_torrent_model_update_stats *trg_torrent_model_update(TrgTorrentModel *model, TrgClient *tc,
                                                         JsonObject *response, gint mode)
{
    GList *torrentList;
    JsonObject *args, *t;
    GList *li;
    gint64 id;
    gint64 serial = trg_client_get_serial(tc);
    JsonArray *removedTorrents;
    GPtrArray *added = g_ptr_array_new_with_free_func(g_object_unref);
    guint whatsChanged = 0;
    guint i;

    gint64 rpcv = trg_client_get_rpc_version(tc);

    args = get_arguments(response);
    torrentList = json_array_get_elements(get_torrents(args));

    model->stats.downRateTotal = 0;
    model->stats.upRateTotal = 0;

    for (li = torrentList; li; li = g_list_next(li)) {
        TrgTorrentItem *item;

        t = json_node_get_object((JsonNode *)li->data);
        id = torrent_get_id(t);

        item = g_hash_table_lookup(model->ht, &id);

        if (!item) {
            gint64 *idCopy = g_new(gint64, 1);

            item = g_object_new(TRG_TYPE_TORRENT_ITEM, NULL);
            update_torrent_item(model, tc, rpcv, serial, item, t, &(model->stats), &whatsChanged);

            *idCopy = id;
            g_hash_table_insert(model->ht, idCopy, g_object_ref(item));
            g_ptr_array_add(added, item);
            whatsChanged |= TORRENT_UPDATE_ADDREMOVE;
        } else {
            update_torrent_item(model, tc, rpcv, serial, item, t, &(model->stats), &whatsChanged);
        }
    }

    g_list_free(torrentList);

    /* all the new torrents in one go */
    if (added->len > 0) {
        GListStore *store = trg_torrent_model_store(model);
        g_list_store_splice(store, g_list_model_get_n_items(G_LIST_MODEL(store)), 0, added->pdata,
                            added->len);
    }

    if (mode != TORRENT_GET_MODE_FIRST)
        for (i = 0; i < added->len; i++)
            g_signal_emit(model, signals[TMODEL_TORRENT_ADDED], 0, added->pdata[i]);

    g_ptr_array_unref(added);

    if (mode == TORRENT_GET_MODE_UPDATE) {
        if (trg_torrent_model_remove_stale(model, serial))
            whatsChanged |= TORRENT_UPDATE_ADDREMOVE;
    } else if (mode > TORRENT_GET_MODE_FIRST) {
        removedTorrents = get_torrents_removed(args);
        if (removedTorrents && json_array_get_length(removedTorrents) > 0) {
            GList *hitlist = json_array_get_elements(removedTorrents);
            for (li = hitlist; li; li = g_list_next(li)) {
                TrgItem *item;
                id = json_node_get_int((JsonNode *)li->data);
                /* mark it, the stale ones are removed below */
                if ((item = g_hash_table_lookup(model->ht, &id)))
                    trg_item_set(item, TORRENT_COLUMN_UPDATESERIAL, (gint64)-1, -1);
            }
            g_list_free(hitlist);

            trg_torrent_model_remove_stale(model, -1);
            whatsChanged |= TORRENT_UPDATE_ADDREMOVE;
        }
    }

    if (whatsChanged != 0) {
        if ((whatsChanged & TORRENT_UPDATE_ADDREMOVE)
            || (whatsChanged & TORRENT_UPDATE_STATE_CHANGE))
            trg_torrent_model_stats_scan(model);
        g_signal_emit(model, signals[TMODEL_STATE_CHANGED], 0, whatsChanged);
    }

    g_signal_emit(model, signals[TMODEL_UPDATE], 0);

    return &(model->stats);
}
