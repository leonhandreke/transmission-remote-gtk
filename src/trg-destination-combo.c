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

#include "torrent.h"
#include "trg-client.h"
#include "trg-destination-combo.h"
#include "trg-item.h"
#include "trg-torrent-model.h"
#include "util.h"

/* An entry for a download directory, with a menu button to choose the default
 * directory, the saved destinations (labels) or the directory of an existing
 * torrent. Emits "changed" when the directory changes. */

struct _TrgDestinationCombo {
    GtkBox parent_instance;

    GtkWidget *entry;
    GtkWidget *button;
    GListStore *store;
    TrgClient *client;
    gchar *last_selection;
};

G_DEFINE_TYPE(TrgDestinationCombo, trg_destination_combo, GTK_TYPE_BOX)

enum {
    PROP_0,
    PROP_CLIENT,
    PROP_LAST_SELECTION
};

enum {
    SIGNAL_CHANGED,
    SIGNAL_COUNT
};

static guint signals[SIGNAL_COUNT] = { 0 };

enum {
    DEST_DEFAULT,
    DEST_LABEL,
    DEST_EXISTING
};

enum {
    DEST_COLUMN_LABEL,
    DEST_COLUMN_DIR,
    DEST_COLUMN_TYPE,
    N_DEST_COLUMNS
};

static const TrgItemColumn destination_item_columns[] = {
    { "label", G_TYPE_STRING },
    { "dir", G_TYPE_STRING },
    { "type", G_TYPE_UINT },
};

#define TRG_TYPE_DESTINATION_ITEM trg_destination_item_get_type()
TRG_DEFINE_ITEM_TYPE(TrgDestinationItem, trg_destination_item, TRG, DESTINATION_ITEM,
                     destination_item_columns)

static void trg_destination_combo_finalize(GObject *object)
{
    TrgDestinationCombo *self = TRG_DESTINATION_COMBO(object);

    g_free(self->last_selection);
    g_clear_object(&self->store);

    G_OBJECT_CLASS(trg_destination_combo_parent_class)->finalize(object);
}

static void trg_destination_combo_get_property(GObject *object, guint property_id, GValue *value,
                                               GParamSpec *pspec)
{
    TrgDestinationCombo *self = TRG_DESTINATION_COMBO(object);

    switch (property_id) {
    case PROP_CLIENT:
        g_value_set_pointer(value, self->client);
        break;
    case PROP_LAST_SELECTION:
        g_value_set_string(value, self->last_selection);
        break;
    default:
        G_OBJECT_WARN_INVALID_PROPERTY_ID(object, property_id, pspec);
        break;
    }
}

static void trg_destination_combo_set_property(GObject *object, guint property_id,
                                               const GValue *value, GParamSpec *pspec)
{
    TrgDestinationCombo *self = TRG_DESTINATION_COMBO(object);

    switch (property_id) {
    case PROP_CLIENT:
        self->client = g_value_get_pointer(value);
        break;
    case PROP_LAST_SELECTION:
        self->last_selection = g_strdup(g_value_get_string(value));
        break;
    default:
        G_OBJECT_WARN_INVALID_PROPERTY_ID(object, property_id, pspec);
        break;
    }
}

gchar *trg_destination_combo_get_dir(TrgDestinationCombo *combo)
{
    return g_strdup(gtk_editable_get_text(GTK_EDITABLE(combo->entry)));
}

void trg_destination_combo_save_selection(TrgDestinationCombo *combo)
{
    if (combo->last_selection && trg_destination_combo_has_text(combo)) {
        TrgPrefs *prefs = trg_client_get_prefs(combo->client);

        trg_prefs_set_string(prefs, combo->last_selection,
                             gtk_editable_get_text(GTK_EDITABLE(combo->entry)),
                             TRG_PREFS_CONNECTION);
    }
}

gboolean trg_destination_combo_has_text(TrgDestinationCombo *combo)
{
    return strlen(gtk_editable_get_text(GTK_EDITABLE(combo->entry))) > 0;
}

static guint trg_destination_combo_find(TrgDestinationCombo *self, const gchar *dir)
{
    GListModel *model = G_LIST_MODEL(self->store);
    guint i, n = g_list_model_get_n_items(model);

    for (i = 0; i < n; i++) {
        g_autoptr(TrgItem) item = g_list_model_get_item(model, i);
        if (!g_strcmp0(trg_item_get_string(item, DEST_COLUMN_DIR), dir))
            return i;
    }

    return GTK_INVALID_LIST_POSITION;
}

static void trg_destination_combo_insert(TrgDestinationCombo *self, const gchar *label,
                                         const gchar *dir, guint type)
{
    gchar *comboLabel;
    TrgItem *item;

    if (type == DEST_EXISTING && trg_destination_combo_find(self, dir) != GTK_INVALID_LIST_POSITION)
        return;

    comboLabel = label ? g_strdup_printf("%s (%s)", label, dir) : g_strdup(dir);

    item = g_object_new(TRG_TYPE_DESTINATION_ITEM, NULL);
    trg_item_set(item, DEST_COLUMN_LABEL, comboLabel, DEST_COLUMN_DIR, dir, DEST_COLUMN_TYPE, type,
                 -1);
    g_list_store_append(self->store, item);
    g_object_unref(item);

    g_free(comboLabel);
}

static void load_directory_model(TrgDestinationCombo *self)
{
    TrgClient *client = self->client;
    TrgPrefs *prefs = trg_client_get_prefs(client);
    GList *li, *list;
    JsonArray *savedDestinations;
    g_autofree gchar *defaultDir = NULL;

    /* Add default dir */
    defaultDir = g_strdup(session_get_download_dir(trg_client_get_session(client)));
    rm_trailing_slashes(defaultDir);

    trg_destination_combo_insert(self, NULL, defaultDir, DEST_DEFAULT);

    /* Add saved dirs */
    savedDestinations
        = trg_prefs_get_array(prefs, TRG_PREFS_KEY_DESTINATIONS, TRG_PREFS_CONNECTION);
    if (savedDestinations) {
        list = json_array_get_elements(savedDestinations);
        for (li = list; li; li = g_list_next(li)) {
            JsonObject *obj = json_node_get_object((JsonNode *)li->data);
            trg_destination_combo_insert(
                self, json_object_get_string_member(obj, TRG_PREFS_SUBKEY_LABEL),
                json_object_get_string_member(obj, TRG_PREFS_KEY_DESTINATIONS_SUBKEY_DIR),
                DEST_LABEL);
        }
        g_list_free(list);
    }

    /* Add all previously used download dirs, sorted */
    list = trg_torrent_model_get_download_dirs(trg_client_get_torrent_table(client));
    list = g_list_sort(list, (GCompareFunc)g_strcmp0);
    for (li = list; li; li = g_list_next(li)) {
        if (g_strcmp0(li->data, defaultDir))
            trg_destination_combo_insert(self, NULL, li->data, DEST_EXISTING);
    }
    g_list_free_full(list, g_free);
}

static void trg_destination_combo_choose(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgDestinationCombo *self = TRG_DESTINATION_COMBO(data);

    gtk_editable_set_text(GTK_EDITABLE(self->entry), g_variant_get_string(parameter, NULL));
}

static void trg_destination_combo_entry_changed_cb(GtkEditable *entry, TrgDestinationCombo *self)
{
    g_signal_emit(self, signals[SIGNAL_CHANGED], 0);
}

/* The menu of the button, choosing an item puts its directory in the entry */
static GMenuModel *trg_destination_combo_menu_new(TrgDestinationCombo *self)
{
    GListModel *model = G_LIST_MODEL(self->store);
    GMenu *menu = g_menu_new();
    guint i, n = g_list_model_get_n_items(model);

    for (i = 0; i < n; i++) {
        g_autoptr(TrgItem) item = g_list_model_get_item(model, i);
        GMenuItem *menu_item = g_menu_item_new(trg_item_get_string(item, DEST_COLUMN_LABEL), NULL);

        g_menu_item_set_action_and_target(menu_item, "destination.choose", "s",
                                          trg_item_get_string(item, DEST_COLUMN_DIR));
        g_menu_append_item(menu, menu_item);
        g_object_unref(menu_item);
    }

    return G_MENU_MODEL(menu);
}

static void trg_destination_combo_constructed(GObject *object)
{
    static const GActionEntry entries[] = {
        { "choose", trg_destination_combo_choose, "s", NULL, NULL, { 0 } },
    };
    TrgDestinationCombo *self = TRG_DESTINATION_COMBO(object);
    TrgPrefs *prefs = trg_client_get_prefs(self->client);
    GSimpleActionGroup *actions = g_simple_action_group_new();
    GMenuModel *menu;
    gchar *lastDestination = NULL;

    G_OBJECT_CLASS(trg_destination_combo_parent_class)->constructed(object);

    load_directory_model(self);

    g_action_map_add_action_entries(G_ACTION_MAP(actions), entries, G_N_ELEMENTS(entries), self);
    gtk_widget_insert_action_group(GTK_WIDGET(self), "destination", G_ACTION_GROUP(actions));
    g_object_unref(actions);

    menu = trg_destination_combo_menu_new(self);
    self->button = gtk_menu_button_new();
    gtk_menu_button_set_menu_model(GTK_MENU_BUTTON(self->button), menu);
    g_object_unref(menu);
    gtk_box_append(GTK_BOX(self), self->button);

    g_signal_connect(self->entry, "changed", G_CALLBACK(trg_destination_combo_entry_changed_cb),
                     self);

    /* Restore any previous selection, or the default dir (the first item) */
    if (self->last_selection)
        lastDestination = trg_prefs_get_string(prefs, self->last_selection, TRG_PREFS_CONNECTION);

    if (lastDestination && *lastDestination) {
        gtk_editable_set_text(GTK_EDITABLE(self->entry), lastDestination);
    } else {
        g_autoptr(TrgItem) item = g_list_model_get_item(G_LIST_MODEL(self->store), 0);
        if (item)
            gtk_editable_set_text(GTK_EDITABLE(self->entry),
                                  trg_item_get_string(item, DEST_COLUMN_DIR));
    }

    g_free(lastDestination);
}

static void trg_destination_combo_class_init(TrgDestinationComboClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);

    object_class->get_property = trg_destination_combo_get_property;
    object_class->set_property = trg_destination_combo_set_property;
    object_class->finalize = trg_destination_combo_finalize;
    object_class->constructed = trg_destination_combo_constructed;

    g_object_class_install_property(
        object_class, PROP_CLIENT,
        g_param_spec_pointer("trg-client", "TClient", "Client",
                             G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_STRINGS));

    g_object_class_install_property(
        object_class, PROP_LAST_SELECTION,
        g_param_spec_string("last-selection-key", "LastSelectionKey", "LastSelectionKey", NULL,
                            G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_STRINGS));

    signals[SIGNAL_CHANGED] = g_signal_new("changed", G_TYPE_FROM_CLASS(klass), G_SIGNAL_RUN_LAST,
                                           0, NULL, NULL, NULL, G_TYPE_NONE, 0);
}

static void trg_destination_combo_init(TrgDestinationCombo *self)
{
    self->store = g_list_store_new(TRG_TYPE_DESTINATION_ITEM);

    gtk_widget_add_css_class(GTK_WIDGET(self), "linked");

    self->entry = gtk_entry_new();
    gtk_entry_set_activates_default(GTK_ENTRY(self->entry), TRUE);
    gtk_widget_set_hexpand(self->entry, TRUE);
    gtk_box_append(GTK_BOX(self), self->entry);
}

GtkWidget *trg_destination_combo_new(TrgClient *client, const gchar *lastSelectionKey)
{
    return GTK_WIDGET(g_object_new(TRG_TYPE_DESTINATION_COMBO, "trg-client", client,
                                   "last-selection-key", lastSelectionKey, NULL));
}
