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
#include <json-glib/json-glib.h>

#include "torrent.h"
#include "trg-client.h"
#include "trg-item.h"
#include "trg-prefs.h"
#include "trg-state-selector.h"
#include "trg-torrent-model.h"
#include "util.h"

enum {
    SELECTOR_STATE_CHANGED,
    SELECTOR_SIGNAL_COUNT
};

enum {
    PROP_0,
    PROP_CLIENT
};

/* The columns of the rows. A row without a name is the separator. */
enum {
    STATE_SELECTOR_ICON,
    STATE_SELECTOR_NAME,
    STATE_SELECTOR_COUNT,
    STATE_SELECTOR_BIT,
    STATE_SELECTOR_SERIAL,
    STATE_SELECTOR_INDEX,
};

static const TrgItemColumn state_item_columns[] = {
    { "icon", G_TYPE_STRING }, { "name", G_TYPE_STRING },  { "count", G_TYPE_INT },
    { "bit", G_TYPE_UINT },    { "serial", G_TYPE_INT64 }, { "index", G_TYPE_UINT },
};

#define TRG_TYPE_STATE_ITEM trg_state_item_get_type()
TRG_DEFINE_ITEM_TYPE(TrgStateItem, trg_state_item, TRG, STATE_ITEM, state_item_columns)

static guint signals[SELECTOR_SIGNAL_COUNT] = { 0 };

struct _TrgStateSelector {
    GtkWidget parent;

    GtkWidget *scrolled;
    GtkWidget *list_view;
    GtkSingleSelection *selection;
    GListStore *store;
    GString *search;
    gint64 search_time;

    guint flag;
    gboolean showDirs;
    gboolean showTrackers;
    gboolean dirsFirst;
    TrgClient *client;
    TrgPrefs *prefs;
    GHashTable *trackers;
    GHashTable *directories;
    GRegex *urlHostRegex;
    gint n_categories;
    TrgItem *error_item;
    TrgItem *all_item;
    TrgItem *paused_item;
    TrgItem *down_item;
    TrgItem *seeding_item;
    TrgItem *complete_item;
    TrgItem *incomplete_item;
    TrgItem *checking_item;
    TrgItem *active_item;
    TrgItem *seed_wait_item;
    TrgItem *down_wait_item;
};

G_DEFINE_TYPE(TrgStateSelector, trg_state_selector, GTK_TYPE_WIDGET)

GRegex *trg_state_selector_get_url_host_regex(TrgStateSelector *s)
{
    return s->urlHostRegex;
}

guint32 trg_state_selector_get_flag(TrgStateSelector *s)
{
    return s->flag;
}

static void state_selection_changed(GtkSingleSelection *selection, GParamSpec *pspec, gpointer data)
{
    TrgStateSelector *self = TRG_STATE_SELECTOR(data);
    TrgItem *item = gtk_single_selection_get_selected_item(selection);

    /* The selected tracker or directory went away, show all torrents */
    if (!item) {
        gtk_single_selection_set_selected(selection, 0);
        return;
    }

    self->flag = trg_item_get_uint(item, STATE_SELECTOR_BIT);

    trg_prefs_set_int(self->prefs, TRG_PREFS_STATE_SELECTOR_LAST,
                      trg_item_get_uint(item, STATE_SELECTOR_INDEX), TRG_PREFS_GLOBAL);

    g_signal_emit(self, signals[SELECTOR_STATE_CHANGED], 0, self->flag);
}

gchar *trg_state_selector_get_selected_text(TrgStateSelector *s)
{
    TrgItem *item = gtk_single_selection_get_selected_item(s->selection);

    return item ? g_strdup(trg_item_get_string(item, STATE_SELECTOR_NAME)) : NULL;
}

static void trg_state_selector_remove_item(TrgStateSelector *s, TrgItem *item)
{
    guint position;

    if (g_list_store_find(s->store, item, &position))
        g_list_store_remove(s->store, position);
}

/* Removes the trackers or directories of table, or those not seen in the
 * update with serial keepSerial if it's not NULL. */
static void trg_state_selector_remove_dynamic(TrgStateSelector *s, GHashTable *table,
                                              const gint64 *keepSerial)
{
    GHashTableIter iter;
    gpointer value;

    g_hash_table_iter_init(&iter, table);
    while (g_hash_table_iter_next(&iter, NULL, &value)) {
        if (!keepSerial || trg_item_get_int64(value, STATE_SELECTOR_SERIAL) != *keepSerial) {
            trg_state_selector_remove_item(s, value);
            g_hash_table_iter_remove(&iter);
        }
    }
}

static void trg_state_selector_update_dynamic_filter(TrgItem *item, gint64 serial)
{
    if (trg_item_get_int64(item, STATE_SELECTOR_SERIAL) != serial)
        trg_item_set(item, STATE_SELECTOR_COUNT, 1, STATE_SELECTOR_SERIAL, serial, -1);
    else
        trg_item_set(item, STATE_SELECTOR_COUNT, trg_item_get_int(item, STATE_SELECTOR_COUNT) + 1,
                     -1);
}

static void refresh_statelist_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgStateSelector *self = TRG_STATE_SELECTOR(data);
    trg_client_inc_serial(self->client);
    trg_state_selector_update(self, TORRENT_UPDATE_ADDREMOVE);
}

static gboolean view_onContextMenu(GtkWidget *widget, gdouble x, gdouble y, gpointer data)
{
    GSimpleActionGroup *actions = g_simple_action_group_new();
    GSimpleAction *action = g_simple_action_new("refresh", NULL);
    GtkRoot *root = gtk_widget_get_root(widget);
    GtkWidget *focus = root ? gtk_root_get_focus(root) : NULL;
    GMenu *menu = g_menu_new();
    GMenuItem *item;
    GIcon *icon;

    /* From the keyboard, point at the row with the focus */
    if ((x < 0 || y < 0) && focus && gtk_widget_is_ancestor(focus, widget)) {
        graphene_rect_t bounds;

        if (gtk_widget_compute_bounds(focus, widget, &bounds)) {
            x = bounds.origin.x + 20;
            y = bounds.origin.y + bounds.size.height / 2;
        }
    }

    g_signal_connect(action, "activate", G_CALLBACK(refresh_statelist_cb), widget);
    g_action_map_add_action(G_ACTION_MAP(actions), G_ACTION(action));
    g_object_unref(action);

    item = g_menu_item_new(_("Refresh"), "state.refresh");
    icon = g_themed_icon_new("view-refresh");
    g_menu_item_set_icon(item, icon);
    g_object_unref(icon);
    g_menu_append_item(menu, item);
    g_object_unref(item);

    trg_popover_menu_popup(widget, G_MENU_MODEL(menu), x, y, "state", G_ACTION_GROUP(actions));

    g_object_unref(menu);
    g_object_unref(actions);

    return TRUE;
}

/* Inserts a row for a tracker or directory, in name order among the range
 * rows after offset (all of them if range is -1). */
static void trg_state_selector_insert(TrgStateSelector *s, gint offset, gint range, TrgItem *item)
{
    GListModel *model = G_LIST_MODEL(s->store);
    const gchar *name = trg_item_get_string(item, STATE_SELECTOR_NAME);
    guint n = g_list_model_get_n_items(model);
    guint pos = MIN((guint)offset, n);
    guint end = range >= 0 ? MIN(n, (guint)(offset + range)) : n;

    for (; pos < end; pos++) {
        g_autoptr(TrgItem) other = g_list_model_get_item(model, pos);
        if (g_strcmp0(trg_item_get_string(other, STATE_SELECTOR_NAME), name) >= 0)
            break;
    }

    g_list_store_insert(s->store, pos, item);
}

static TrgItem *trg_state_selector_filter_item_new(const gchar *icon, const gchar *name,
                                                   guint32 bit, gint64 serial)
{
    TrgItem *item = g_object_new(TRG_TYPE_STATE_ITEM, NULL);

    trg_item_set(item, STATE_SELECTOR_ICON, icon, STATE_SELECTOR_NAME, name, STATE_SELECTOR_SERIAL,
                 serial, STATE_SELECTOR_COUNT, 1, STATE_SELECTOR_BIT, bit, STATE_SELECTOR_INDEX, 0,
                 -1);

    return item;
}

/* Adds, counts and removes the tracker and/or directory rows */
static void trg_state_selector_update_filters(TrgStateSelector *s, gboolean updateTrackers,
                                              gboolean updateDirs)
{
    TrgClient *client = s->client;
    gint64 updateSerial = trg_client_get_serial(client);
    GList *torrentItems, *trackersList, *trackerItem, *li;
    TrgItem *item;

    if (!trg_client_is_connected(client) || !(updateTrackers || updateDirs))
        return;

    torrentItems = g_hash_table_get_values(trg_client_get_torrent_table(client));

    for (li = torrentItems; li; li = g_list_next(li)) {
        TrgTorrentItem *torrent = li->data;
        JsonObject *t = trg_torrent_item_get_json(torrent);

        if (!t)
            continue;

        if (updateTrackers) {
            trackersList = json_array_get_elements(torrent_get_tracker_stats(t));
            for (trackerItem = trackersList; trackerItem; trackerItem = g_list_next(trackerItem)) {
                JsonObject *tracker = json_node_get_object((JsonNode *)trackerItem->data);
                const gchar *announceUrl = tracker_stats_get_announce(tracker);
                gchar *announceHost = trg_gregex_get_first(s->urlHostRegex, announceUrl);

                if (!announceHost)
                    continue;

                item = g_hash_table_lookup(s->trackers, announceHost);

                if (item) {
                    trg_state_selector_update_dynamic_filter(item, updateSerial);
                    g_free(announceHost);
                    continue;
                }

                item = trg_state_selector_filter_item_new("network-workgroup", announceHost,
                                                          FILTER_FLAG_TRACKER, updateSerial);

                if (s->dirsFirst)
                    trg_state_selector_insert(
                        s, s->n_categories + g_hash_table_size(s->directories), -1, item);
                else
                    trg_state_selector_insert(s, s->n_categories, g_hash_table_size(s->trackers),
                                              item);

                g_hash_table_insert(s->trackers, announceHost, item);
            }
            g_list_free(trackersList);
        }

        if (updateDirs) {
            const gchar *dir = trg_item_get_string(torrent, TORRENT_COLUMN_DOWNLOADDIR_SHORT);

            if (!dir)
                continue;

            item = g_hash_table_lookup(s->directories, dir);

            if (item) {
                trg_state_selector_update_dynamic_filter(item, updateSerial);
                continue;
            }

            item = trg_state_selector_filter_item_new("folder", dir, FILTER_FLAG_DIR, updateSerial);

            if (s->dirsFirst)
                trg_state_selector_insert(s, s->n_categories, g_hash_table_size(s->directories),
                                          item);
            else
                trg_state_selector_insert(s, s->n_categories + g_hash_table_size(s->trackers), -1,
                                          item);

            g_hash_table_insert(s->directories, g_strdup(dir), item);
        }
    }

    g_list_free(torrentItems);

    if (updateTrackers)
        trg_state_selector_remove_dynamic(s, s->trackers, &updateSerial);

    if (updateDirs)
        trg_state_selector_remove_dynamic(s, s->directories, &updateSerial);
}

void trg_state_selector_update(TrgStateSelector *s, guint whatsChanged)
{
    trg_state_selector_update_filters(
        s, s->showTrackers && (whatsChanged & TORRENT_UPDATE_ADDREMOVE),
        s->showDirs && (whatsChanged & (TORRENT_UPDATE_ADDREMOVE | TORRENT_UPDATE_PATH_CHANGE)));
}

void trg_state_selector_set_show_dirs(TrgStateSelector *s, gboolean show)
{
    s->showDirs = show;
    /* rebuilt, the rows count each torrent once per update */
    trg_state_selector_remove_dynamic(s, s->directories, NULL);
    if (show)
        trg_state_selector_update_filters(s, FALSE, TRUE);
}

static void on_torrents_state_change(TrgTorrentModel *model, guint whatsChanged, gpointer data)
{
    TrgStateSelector *selector = TRG_STATE_SELECTOR(data);
    trg_state_selector_update(selector, whatsChanged);

    if ((whatsChanged & TORRENT_UPDATE_ADDREMOVE) || (whatsChanged & TORRENT_UPDATE_STATE_CHANGE))
        trg_state_selector_stats_update(selector, trg_torrent_model_get_stats(model));
}

void trg_state_selector_set_show_trackers(TrgStateSelector *s, gboolean show)
{
    s->showTrackers = show;
    /* rebuilt, the rows count each torrent once per update */
    trg_state_selector_remove_dynamic(s, s->trackers, NULL);
    if (show)
        trg_state_selector_update_filters(s, TRUE, FALSE);
}

void trg_state_selector_set_directories_first(TrgStateSelector *s, gboolean _dirsFirst)
{
    s->dirsFirst = _dirsFirst;
    trg_state_selector_remove_dynamic(s, s->directories, NULL);
    trg_state_selector_remove_dynamic(s, s->trackers, NULL);
    trg_state_selector_update_filters(s, s->showTrackers, s->showDirs);
}

/* Adds a state row at pos (or at the end if pos < 0); its index (for
 * restoring the selection) is the number of rows before it was added. */
static void trg_state_selector_add_state(TrgStateSelector *selector, gint pos, const gchar *icon,
                                         const gchar *name, guint32 flag, TrgItem **out)
{
    TrgItem *item = g_object_new(TRG_TYPE_STATE_ITEM, NULL);
    guint n = g_list_model_get_n_items(G_LIST_MODEL(selector->store));

    trg_item_set(item, STATE_SELECTOR_ICON, icon, STATE_SELECTOR_NAME, name, STATE_SELECTOR_BIT,
                 flag, STATE_SELECTOR_INDEX, n, -1);

    if (pos < 0)
        g_list_store_append(selector->store, item);
    else
        g_list_store_insert(selector->store, pos, item);

    if (out)
        *out = item;

    g_object_unref(item);

    selector->n_categories++;
}

static void trg_state_selector_remove_state(TrgStateSelector *s, TrgItem **item)
{
    if (!*item)
        return;

    trg_state_selector_remove_item(s, *item);
    *item = NULL;
    s->n_categories--;
}

static void trg_state_selector_update_stat(TrgItem *item, gint count)
{
    if (item)
        trg_item_set(item, STATE_SELECTOR_COUNT, count, -1);
}

void trg_state_selector_stats_update(TrgStateSelector *s, trg_torrent_model_update_stats *stats)
{
    if (stats->error > 0 && !s->error_item)
        trg_state_selector_add_state(s, s->n_categories - 1, "dialog-warning", _("Error"),
                                     TORRENT_FLAG_ERROR, &s->error_item);
    else if (stats->error < 1 && s->error_item)
        trg_state_selector_remove_state(s, &s->error_item);

    trg_state_selector_update_stat(s->all_item, stats->count);
    trg_state_selector_update_stat(s->down_item, stats->down);
    trg_state_selector_update_stat(s->seeding_item, stats->seeding);
    trg_state_selector_update_stat(s->error_item, stats->error);
    trg_state_selector_update_stat(s->paused_item, stats->paused);
    trg_state_selector_update_stat(s->complete_item, stats->complete);
    trg_state_selector_update_stat(s->incomplete_item, stats->incomplete);
    trg_state_selector_update_stat(s->active_item, stats->active);
    trg_state_selector_update_stat(s->checking_item, stats->checking);
    trg_state_selector_update_stat(s->down_wait_item, stats->down_wait);
    trg_state_selector_update_stat(s->seed_wait_item, stats->seed_wait);
}

void trg_state_selector_disconnect(TrgStateSelector *s)
{
    trg_state_selector_remove_state(s, &s->error_item);
    trg_state_selector_remove_dynamic(s, s->trackers, NULL);
    trg_state_selector_remove_dynamic(s, s->directories, NULL);

    trg_state_selector_update_stat(s->all_item, -1);
    trg_state_selector_update_stat(s->down_item, -1);
    trg_state_selector_update_stat(s->seeding_item, -1);
    trg_state_selector_update_stat(s->paused_item, -1);
    trg_state_selector_update_stat(s->complete_item, -1);
    trg_state_selector_update_stat(s->incomplete_item, -1);
    trg_state_selector_update_stat(s->active_item, -1);
    trg_state_selector_update_stat(s->checking_item, -1);
    trg_state_selector_update_stat(s->down_wait_item, -1);
    trg_state_selector_update_stat(s->seed_wait_item, -1);
}

void trg_state_selector_set_queues_enabled(TrgStateSelector *s, gboolean enabled)
{
    if (enabled) {
        trg_state_selector_add_state(s, 2, "media-seek-backward", _("Queue Down"),
                                     TORRENT_FLAG_DOWNLOADING_WAIT, &s->down_wait_item);
        trg_state_selector_add_state(s, 4, "media-seek-forward", _("Queue Up"),
                                     TORRENT_FLAG_SEEDING_WAIT, &s->seed_wait_item);
        /* their indices when they were added at the start */
        trg_item_set(s->down_wait_item, STATE_SELECTOR_INDEX, 2, -1);
        trg_item_set(s->seed_wait_item, STATE_SELECTOR_INDEX, 4, -1);
    } else {
        trg_state_selector_remove_state(s, &s->seed_wait_item);
        trg_state_selector_remove_state(s, &s->down_wait_item);
    }
}

/* Selects the row at position, and moves the focus to it */
static void trg_state_selector_select(TrgStateSelector *s, guint position)
{
#if GTK_CHECK_VERSION(4, 12, 0)
    gtk_list_view_scroll_to(GTK_LIST_VIEW(s->list_view), position,
                            GTK_LIST_SCROLL_FOCUS | GTK_LIST_SCROLL_SELECT, NULL);
#else
    gtk_single_selection_set_selected(s->selection, position);
    gtk_widget_activate_action(s->list_view, "list.scroll-to-item", "u", position);
#endif
}

/***
 * Type ahead search: typing selects the first row whose name starts with the
 * typed text.
 */

static gboolean trg_state_selector_key_pressed(GtkEventControllerKey *controller, guint keyval,
                                               guint keycode, GdkModifierType state,
                                               TrgStateSelector *s)
{
    GListModel *model = G_LIST_MODEL(s->store);
    gunichar c = gdk_keyval_to_unicode(keyval);
    gint64 now = g_get_monotonic_time();
    guint i, n = g_list_model_get_n_items(model);
    gchar *needle;

    if (!c || !g_unichar_isgraph(c)
        || (state & (GDK_CONTROL_MASK | GDK_ALT_MASK | GDK_META_MASK | GDK_SUPER_MASK)))
        return FALSE;

    /* a pause starts a new search */
    if (now - s->search_time > G_USEC_PER_SEC)
        g_string_truncate(s->search, 0);
    s->search_time = now;

    g_string_append_unichar(s->search, c);
    needle = g_utf8_casefold(s->search->str, -1);

    for (i = 0; i < n; i++) {
        g_autoptr(TrgItem) item = g_list_model_get_item(model, i);
        const gchar *name = trg_item_get_string(item, STATE_SELECTOR_NAME);
        gchar *folded = name ? g_utf8_casefold(name, -1) : NULL;
        gboolean found = folded && g_str_has_prefix(folded, needle);

        g_free(folded);

        if (found) {
            trg_state_selector_select(s, i);
            break;
        }
    }

    g_free(needle);

    return TRUE;
}

/* The position of the row with the focus, or GTK_INVALID_LIST_POSITION */
static guint trg_state_selector_get_focus_position(TrgStateSelector *s)
{
    GtkRoot *root = gtk_widget_get_root(s->list_view);
    GtkWidget *w = root ? gtk_root_get_focus(root) : NULL;

    if (!w || !gtk_widget_is_ancestor(w, s->list_view))
        return GTK_INVALID_LIST_POSITION;

    /* up to the row widget, a child of the list view, then down to ours */
    while (gtk_widget_get_parent(w) != s->list_view)
        w = gtk_widget_get_parent(w);

    for (w = gtk_widget_get_first_child(w); w; w = gtk_widget_get_next_sibling(w)) {
        GtkListItem *list_item = g_object_get_data(G_OBJECT(w), "trg-list-item");
        if (list_item)
            return gtk_list_item_get_position(list_item);
    }

    return GTK_INVALID_LIST_POSITION;
}

/* Moving the cursor up or down onto the separator moves it past it */
static gboolean trg_state_selector_skip_separator(GtkEventControllerKey *controller, guint keyval,
                                                  guint keycode, GdkModifierType state,
                                                  TrgStateSelector *s)
{
    GListModel *model = G_LIST_MODEL(s->store);
    guint n = g_list_model_get_n_items(model);
    guint pos, target;
    gint step;
    g_autoptr(TrgItem) item = NULL;

    if (keyval == GDK_KEY_Up || keyval == GDK_KEY_KP_Up)
        step = -1;
    else if (keyval == GDK_KEY_Down || keyval == GDK_KEY_KP_Down)
        step = 1;
    else
        return FALSE;

    if (state & GDK_MODIFIER_MASK)
        return FALSE;

    pos = trg_state_selector_get_focus_position(s);
    if (pos == GTK_INVALID_LIST_POSITION || (step < 0 && pos < 2) || pos + 2 * step >= n)
        return FALSE;

    item = g_list_model_get_item(model, pos + step);
    if (trg_item_get_string(item, STATE_SELECTOR_NAME))
        return FALSE;

    target = pos + 2 * step;
    trg_state_selector_select(s, target);

    return TRUE;
}

/***
 * The rows: an icon, the name and the count, or a separator
 */

static void trg_state_selector_row_refresh(GtkListItem *list_item)
{
    TrgItem *item = gtk_list_item_get_item(list_item);
    GtkWidget *image = gtk_widget_get_first_child(gtk_list_item_get_child(list_item));
    GtkWidget *name_label = gtk_widget_get_next_sibling(image);
    GtkWidget *count_label = gtk_widget_get_next_sibling(name_label);
    gint count = trg_item_get_int(item, STATE_SELECTOR_COUNT);

    gtk_image_set_from_icon_name(GTK_IMAGE(image), trg_item_get_string(item, STATE_SELECTOR_ICON));
    gtk_label_set_text(GTK_LABEL(name_label), trg_item_get_string(item, STATE_SELECTOR_NAME));

    if (count > 0) {
        gchar *text = g_strdup_printf("%d", count);
        gtk_label_set_text(GTK_LABEL(count_label), text);
        g_free(text);
    }
    gtk_widget_set_visible(count_label, count > 0);
}

static void trg_state_selector_row_setup(GtkSignalListItemFactory *factory, GtkListItem *list_item,
                                         gpointer data)
{
    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
    GtkWidget *label = gtk_label_new(NULL);
    GtkWidget *separator;

    gtk_box_append(GTK_BOX(box), gtk_image_new());

    gtk_label_set_xalign(GTK_LABEL(label), 0);
    gtk_widget_set_hexpand(label, TRUE);
    gtk_box_append(GTK_BOX(box), label);

    label = gtk_label_new(NULL);
    gtk_widget_add_css_class(label, "dim-label");
    gtk_widget_add_css_class(label, "numeric");
    gtk_box_append(GTK_BOX(box), label);

    separator = gtk_separator_new(GTK_ORIENTATION_HORIZONTAL);
    gtk_widget_set_hexpand(separator, TRUE);
    gtk_widget_set_valign(separator, GTK_ALIGN_CENTER);
    gtk_box_append(GTK_BOX(box), separator);

    g_object_set_data(G_OBJECT(box), "trg-list-item", list_item);
    gtk_list_item_set_child(list_item, box);
}

static void trg_state_selector_row_notify(GObject *object, GParamSpec *pspec,
                                          GtkListItem *list_item)
{
    trg_state_selector_row_refresh(list_item);
}

static void trg_state_selector_row_bind(GtkSignalListItemFactory *factory, GtkListItem *list_item,
                                        gpointer data)
{
    TrgItem *item = gtk_list_item_get_item(list_item);
    gboolean separator = trg_item_get_string(item, STATE_SELECTOR_NAME) == NULL;
    GtkWidget *child;

    /* the separator row (between the states and the other filters) shows
     * only its separator, and can't be selected */
    for (child = gtk_widget_get_first_child(gtk_list_item_get_child(list_item)); child;
         child = gtk_widget_get_next_sibling(child))
        gtk_widget_set_visible(child, GTK_IS_SEPARATOR(child) == separator);

    gtk_list_item_set_selectable(list_item, !separator);
    gtk_list_item_set_activatable(list_item, !separator);
#if GTK_CHECK_VERSION(4, 12, 0)
    gtk_list_item_set_focusable(list_item, !separator);
#endif

    if (separator)
        return;

    g_signal_connect(item, "notify", G_CALLBACK(trg_state_selector_row_notify), list_item);
    trg_state_selector_row_refresh(list_item);
}

static void trg_state_selector_row_unbind(GtkSignalListItemFactory *factory, GtkListItem *list_item,
                                          gpointer data)
{
    TrgItem *item = gtk_list_item_get_item(list_item);

    if (item)
        g_signal_handlers_disconnect_by_func(item, trg_state_selector_row_notify, list_item);
}

TrgStateSelector *trg_state_selector_new(TrgClient *client, TrgTorrentModel *tmodel)
{
    TrgStateSelector *selector = g_object_new(TRG_TYPE_STATE_SELECTOR, "client", client, NULL);
    g_signal_connect_object(tmodel, "torrents-state-change", G_CALLBACK(on_torrents_state_change),
                            selector, 0);
    return selector;
}

static void trg_state_selector_constructed(GObject *object)
{
    TrgStateSelector *selector = TRG_STATE_SELECTOR(object);
    GtkListItemFactory *factory;
    GtkEventController *keys;
    gint index;

    G_OBJECT_CLASS(trg_state_selector_parent_class)->constructed(object);

    selector->showDirs
        = trg_prefs_get_bool(selector->prefs, TRG_PREFS_KEY_FILTER_DIRS, TRG_PREFS_GLOBAL);
    selector->showTrackers
        = trg_prefs_get_bool(selector->prefs, TRG_PREFS_KEY_FILTER_TRACKERS, TRG_PREFS_GLOBAL);
    selector->dirsFirst
        = trg_prefs_get_bool(selector->prefs, TRG_PREFS_KEY_DIRECTORIES_FIRST, TRG_PREFS_GLOBAL);

    trg_state_selector_add_state(selector, -1, "help-about", _("All"), 0, &selector->all_item);
    trg_state_selector_add_state(selector, -1, "go-down", _("Downloading"),
                                 TORRENT_FLAG_DOWNLOADING, &selector->down_item);
    trg_state_selector_add_state(selector, -1, "media-seek-backward", _("Queue Down"),
                                 TORRENT_FLAG_DOWNLOADING_WAIT, &selector->down_wait_item);
    trg_state_selector_add_state(selector, -1, "go-up", _("Seeding"), TORRENT_FLAG_SEEDING,
                                 &selector->seeding_item);
    trg_state_selector_add_state(selector, -1, "media-seek-forward", _("Queue Up"),
                                 TORRENT_FLAG_SEEDING_WAIT, &selector->seed_wait_item);
    trg_state_selector_add_state(selector, -1, "media-playback-pause", _("Paused"),
                                 TORRENT_FLAG_PAUSED, &selector->paused_item);
    trg_state_selector_add_state(selector, -1, "trg-gtk-apply", _("Complete"),
                                 TORRENT_FLAG_COMPLETE, &selector->complete_item);
    trg_state_selector_add_state(selector, -1, "edit-select-all", _("Incomplete"),
                                 TORRENT_FLAG_INCOMPLETE, &selector->incomplete_item);
    trg_state_selector_add_state(selector, -1, "network-workgroup", _("Active"),
                                 TORRENT_FLAG_ACTIVE, &selector->active_item);
    trg_state_selector_add_state(selector, -1, "view-refresh", _("Checking"),
                                 TORRENT_FLAG_CHECKING_ANY, &selector->checking_item);
    trg_state_selector_add_state(selector, -1, NULL, NULL, 0, NULL);

    /* Always one filter selected, "All" by default */
    selector->selection = gtk_single_selection_new(G_LIST_MODEL(g_object_ref(selector->store)));
    gtk_single_selection_set_autoselect(selector->selection, FALSE);
    gtk_single_selection_set_can_unselect(selector->selection, FALSE);
    g_signal_connect(selector->selection, "notify::selected-item",
                     G_CALLBACK(state_selection_changed), selector);

    index = trg_prefs_get_int(selector->prefs, TRG_PREFS_STATE_SELECTOR_LAST, TRG_PREFS_GLOBAL);
    if (index < 0 || index >= selector->n_categories - 1)
        index = 0;
    gtk_single_selection_set_selected(selector->selection, index);

    factory = gtk_signal_list_item_factory_new();
    g_signal_connect(factory, "setup", G_CALLBACK(trg_state_selector_row_setup), NULL);
    g_signal_connect(factory, "bind", G_CALLBACK(trg_state_selector_row_bind), NULL);
    g_signal_connect(factory, "unbind", G_CALLBACK(trg_state_selector_row_unbind), NULL);

    selector->list_view
        = gtk_list_view_new(GTK_SELECTION_MODEL(g_object_ref(selector->selection)), factory);
    gtk_widget_add_css_class(selector->list_view, "navigation-sidebar");

    keys = gtk_event_controller_key_new();
    g_signal_connect(keys, "key-pressed", G_CALLBACK(trg_state_selector_key_pressed), selector);
    gtk_widget_add_controller(selector->list_view, keys);

    /* before the list view's own cursor keys */
    keys = gtk_event_controller_key_new();
    gtk_event_controller_set_propagation_phase(keys, GTK_PHASE_CAPTURE);
    g_signal_connect(keys, "key-pressed", G_CALLBACK(trg_state_selector_skip_separator), selector);
    gtk_widget_add_controller(selector->list_view, keys);

    selector->scrolled = my_scrolledwin_new(selector->list_view);
    /* the sidebar is as wide as the names (at least) */
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(selector->scrolled), GTK_POLICY_NEVER,
                                   GTK_POLICY_AUTOMATIC);
    gtk_widget_set_parent(selector->scrolled, GTK_WIDGET(selector));

    trg_widget_add_context_menu(GTK_WIDGET(selector), view_onContextMenu, NULL);
}

static void trg_state_selector_get_property(GObject *object, guint property_id, GValue *value,
                                            GParamSpec *pspec)
{
    TrgStateSelector *self = TRG_STATE_SELECTOR(object);
    switch (property_id) {
    case PROP_CLIENT:
        g_value_set_object(value, self->client);
        break;
    default:
        G_OBJECT_WARN_INVALID_PROPERTY_ID(object, property_id, pspec);
        break;
    }
}

static void trg_state_selector_set_property(GObject *object, guint prop_id, const GValue *value,
                                            GParamSpec *pspec G_GNUC_UNUSED)
{
    TrgStateSelector *self = TRG_STATE_SELECTOR(object);
    switch (prop_id) {
    case PROP_CLIENT:
        self->client = g_value_get_object(value);
        self->prefs = trg_client_get_prefs(self->client);
        break;
    }
}

static void trg_state_selector_measure(GtkWidget *widget, GtkOrientation orientation, int for_size,
                                       int *minimum, int *natural, int *minimum_baseline,
                                       int *natural_baseline)
{
    gtk_widget_measure(TRG_STATE_SELECTOR(widget)->scrolled, orientation, for_size, minimum,
                       natural, minimum_baseline, natural_baseline);
}

/* The context menu is a popover of the selector, present it */
static void trg_state_selector_size_allocate(GtkWidget *widget, int width, int height, int baseline)
{
    GtkWidget *child;

    gtk_widget_allocate(TRG_STATE_SELECTOR(widget)->scrolled, width, height, baseline, NULL);

    for (child = gtk_widget_get_first_child(widget); child;
         child = gtk_widget_get_next_sibling(child)) {
        if (GTK_IS_POPOVER(child))
            gtk_popover_present(GTK_POPOVER(child));
    }
}

static gboolean trg_state_selector_focus(GtkWidget *widget, GtkDirectionType direction)
{
    return gtk_widget_child_focus(TRG_STATE_SELECTOR(widget)->scrolled, direction);
}

static gboolean trg_state_selector_grab_focus(GtkWidget *widget)
{
    return gtk_widget_grab_focus(TRG_STATE_SELECTOR(widget)->list_view);
}

static void trg_state_selector_dispose(GObject *object)
{
    TrgStateSelector *self = TRG_STATE_SELECTOR(object);
    GtkWidget *child;

    if (self->selection)
        g_signal_handlers_disconnect_by_data(self->selection, self);

    while ((child = gtk_widget_get_first_child(GTK_WIDGET(object))))
        gtk_widget_unparent(child);

    g_clear_pointer(&self->trackers, g_hash_table_destroy);
    g_clear_pointer(&self->directories, g_hash_table_destroy);
    g_clear_object(&self->selection);
    g_clear_object(&self->store);

    G_OBJECT_CLASS(trg_state_selector_parent_class)->dispose(object);
}

static void trg_state_selector_finalize(GObject *object)
{
    TrgStateSelector *self = TRG_STATE_SELECTOR(object);

    g_regex_unref(self->urlHostRegex);
    g_string_free(self->search, TRUE);

    G_OBJECT_CLASS(trg_state_selector_parent_class)->finalize(object);
}

static void trg_state_selector_class_init(TrgStateSelectorClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);
    GtkWidgetClass *widget_class = GTK_WIDGET_CLASS(klass);

    object_class->constructed = trg_state_selector_constructed;
    object_class->set_property = trg_state_selector_set_property;
    object_class->get_property = trg_state_selector_get_property;
    object_class->dispose = trg_state_selector_dispose;
    object_class->finalize = trg_state_selector_finalize;

    widget_class->measure = trg_state_selector_measure;
    widget_class->size_allocate = trg_state_selector_size_allocate;
    widget_class->focus = trg_state_selector_focus;
    widget_class->grab_focus = trg_state_selector_grab_focus;

    signals[SELECTOR_STATE_CHANGED]
        = g_signal_new("torrent-state-changed", G_TYPE_FROM_CLASS(object_class),
                       G_SIGNAL_RUN_LAST | G_SIGNAL_ACTION, 0, NULL, NULL,
                       g_cclosure_marshal_VOID__UINT, G_TYPE_NONE, 1, G_TYPE_UINT);

    g_object_class_install_property(
        object_class, PROP_CLIENT,
        g_param_spec_object("client", "Client", "Client", TRG_TYPE_CLIENT,
                            G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));
}

static void trg_state_selector_init(TrgStateSelector *self)
{
    self->urlHostRegex = trg_uri_host_regex_new();
    self->trackers = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, g_object_unref);
    self->directories = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, g_object_unref);
    self->store = g_list_store_new(TRG_TYPE_STATE_ITEM);
    self->search = g_string_new(NULL);
}
