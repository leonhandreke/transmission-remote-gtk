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

#include "trg-item.h"
#include "trg-persistent-tree-view.h"
#include "trg-preferences-dialog.h"
#include "trg-prefs.h"
#include "trg-tree-view.h"
#include "util.h"

/* A config list which can save/restore simple data into the TrgConf backend.
 * It's a GtkBox which contains a TrgTreeView of editable text columns and the
 * buttons to add/delete/move entries.
 */

static const TrgItemColumn persistent_item_columns[TRG_PERSISTENT_TREE_VIEW_MAX_COLUMNS] = {
    { "column-0", G_TYPE_STRING },
    { "column-1", G_TYPE_STRING },
};

#define TRG_TYPE_PERSISTENT_ITEM trg_persistent_item_get_type()
TRG_DEFINE_ITEM_TYPE(TrgPersistentItem, trg_persistent_item, TRG, PERSISTENT_ITEM,
                     persistent_item_columns)

struct _trg_persistent_tree_view_column {
    gchar *key;
    gint index;
    trg_column_description *desc;
};

struct _TrgPersistentTreeView {
    GtkBox parent;

    GSList *columns;
    TrgTreeView *tv;
    GListStore *store;
    GtkWidget *delButton;
    GtkWidget *upButton;
    GtkWidget *downButton;
    trg_pref_widget_desc *wd;
    trg_persistent_tree_view_column *addSelect;
};

G_DEFINE_TYPE(TrgPersistentTreeView, trg_persistent_tree_view, GTK_TYPE_BOX)

/* The position of the only selected entry in the store, or
 * GTK_INVALID_LIST_POSITION */
static guint trg_persistent_tree_view_selected_position(TrgPersistentTreeView *ptv)
{
    guint position;

    if (trg_tree_view_count_selected(ptv->tv) == 1
        && g_list_store_find(ptv->store, trg_tree_view_get_first_selected(ptv->tv), &position))
        return position;

    return GTK_INVALID_LIST_POSITION;
}

static void trg_persistent_tree_view_update_buttons(TrgPersistentTreeView *ptv)
{
    guint position = trg_persistent_tree_view_selected_position(ptv);
    guint n = g_list_model_get_n_items(G_LIST_MODEL(ptv->store));

    gtk_widget_set_sensitive(ptv->delButton, trg_tree_view_count_selected(ptv->tv) > 0);
    gtk_widget_set_sensitive(ptv->upButton, position != GTK_INVALID_LIST_POSITION && position > 0);
    gtk_widget_set_sensitive(ptv->downButton,
                             position != GTK_INVALID_LIST_POSITION && position + 1 < n);
}

static void trg_persistent_tree_view_selection_changed(GtkSelectionModel *selection, guint position,
                                                       guint n_items, gpointer data)
{
    trg_persistent_tree_view_update_buttons(TRG_PERSISTENT_TREE_VIEW(data));
}

static void trg_persistent_tree_view_items_changed(GListModel *model, guint position, guint removed,
                                                   guint added, gpointer data)
{
    trg_persistent_tree_view_update_buttons(TRG_PERSISTENT_TREE_VIEW(data));
}

static void trg_persistent_tree_view_edit(TrgTreeView *tv, gpointer item, gboolean done,
                                          const gchar *new_text, gpointer data)
{
    trg_persistent_tree_view_column *cd = data;

    if (done && new_text)
        trg_item_set(item, cd->index, new_text, -1);
}

static void trg_persistent_tree_view_refresh(TrgPrefs *prefs, void *wdp)
{
    trg_pref_widget_desc *wd = (trg_pref_widget_desc *)wdp;
    TrgPersistentTreeView *ptv = TRG_PERSISTENT_TREE_VIEW(wd->widget);
    JsonArray *ja = trg_prefs_get_array(prefs, wd->key, wd->flags);
    GPtrArray *items = g_ptr_array_new_with_free_func(g_object_unref);
    GList *ja_list, *li;
    GSList *sli;

    ja_list = ja ? json_array_get_elements(ja) : NULL;

    for (li = ja_list; li; li = g_list_next(li)) {
        JsonObject *jobj = json_node_get_object((JsonNode *)li->data);
        TrgItem *item = g_object_new(TRG_TYPE_PERSISTENT_ITEM, NULL);

        for (sli = ptv->columns; sli; sli = g_slist_next(sli)) {
            trg_persistent_tree_view_column *cd = sli->data;
            trg_item_set(item, cd->index, json_object_get_string_member(jobj, cd->key), -1);
        }

        g_ptr_array_add(items, item);
    }

    g_list_free(ja_list);

    g_list_store_splice(ptv->store, 0, g_list_model_get_n_items(G_LIST_MODEL(ptv->store)),
                        items->pdata, items->len);
    g_ptr_array_unref(items);
}

static void trg_persistent_tree_view_save(TrgPrefs *prefs, void *wdp)
{
    trg_pref_widget_desc *wd = (trg_pref_widget_desc *)wdp;
    TrgPersistentTreeView *ptv = TRG_PERSISTENT_TREE_VIEW(wd->widget);
    GListModel *model = G_LIST_MODEL(ptv->store);
    JsonNode *node;
    JsonArray *ja;
    guint i, n;

    /* keep an entry that is still being edited, like when OK is clicked */
    trg_tree_view_stop_editing(ptv->tv);

    node = trg_prefs_get_value(prefs, wd->key, JSON_NODE_ARRAY, wd->flags | TRG_PREFS_REPLACENODE);
    ja = json_array_new();
    n = g_list_model_get_n_items(model);

    for (i = 0; i < n; i++) {
        g_autoptr(TrgItem) item = g_list_model_get_item(model, i);
        JsonObject *new = json_object_new();
        GSList *li;

        for (li = ptv->columns; li; li = g_slist_next(li)) {
            trg_persistent_tree_view_column *cd = li->data;
            json_object_set_string_member(new, cd->key, trg_item_get_string(item, cd->index));
        }

        json_array_add_object_element(ja, new);
    }

    json_node_take_array(node, ja);

    trg_prefs_changed_emit_signal(prefs, wd->key);
}

trg_persistent_tree_view_column *trg_persistent_tree_view_add_column(TrgPersistentTreeView *ptv,
                                                                     gint index, const gchar *key,
                                                                     const gchar *label)
{
    trg_persistent_tree_view_column *cd;

    g_return_val_if_fail(index >= 0 && index < TRG_PERSISTENT_TREE_VIEW_MAX_COLUMNS, NULL);

    cd = g_new0(trg_persistent_tree_view_column, 1);
    cd->key = g_strdup(key);
    cd->index = index;
    cd->desc = trg_tree_view_reg_column(ptv->tv, TRG_COLTYPE_TEXT, index, label, key,
                                        TRG_COLUMN_UNREMOVABLE | TRG_COLUMN_EDITABLE);
    cd->desc->edit = trg_persistent_tree_view_edit;
    cd->desc->func_data = cd;

    ptv->columns = g_slist_append(ptv->columns, cd);

    trg_tree_view_remove_all_columns(ptv->tv);
    trg_tree_view_setup_columns(ptv->tv);

    return cd;
}

static void trg_persistent_tree_view_edit_item(TrgPersistentTreeView *ptv, TrgItem *item)
{
    trg_persistent_tree_view_column *cd
        = ptv->addSelect ? ptv->addSelect : (ptv->columns ? ptv->columns->data : NULL);

    trg_tree_view_select_item(ptv->tv, item);

    if (cd)
        trg_tree_view_start_editing(ptv->tv, item, cd->desc);
}

static void trg_persistent_tree_view_add_cb(GtkWidget *w, gpointer data)
{
    TrgPersistentTreeView *ptv = TRG_PERSISTENT_TREE_VIEW(data);
    TrgItem *item = g_object_new(TRG_TYPE_PERSISTENT_ITEM, NULL);

    g_list_store_append(ptv->store, item);
    trg_persistent_tree_view_edit_item(ptv, item);
    g_object_unref(item);
}

/* Enter or a double click on a row edits it */
static void trg_persistent_tree_view_row_activated(TrgTreeView *tv, guint position, gpointer data)
{
    g_autoptr(GObject) row
        = g_list_model_get_item(G_LIST_MODEL(trg_tree_view_get_selection(tv)), position);

    if (row)
        trg_persistent_tree_view_edit_item(TRG_PERSISTENT_TREE_VIEW(data),
                                           trg_tree_view_row_item(row));
}

static void trg_persistent_tree_view_move(TrgPersistentTreeView *ptv, gint offset)
{
    guint position = trg_persistent_tree_view_selected_position(ptv);
    guint n = g_list_model_get_n_items(G_LIST_MODEL(ptv->store));
    TrgItem *item;

    if (position == GTK_INVALID_LIST_POSITION || (offset < 0 && position == 0)
        || (offset > 0 && position + 1 >= n))
        return;

    item = g_list_model_get_item(G_LIST_MODEL(ptv->store), position);
    g_list_store_remove(ptv->store, position);
    g_list_store_insert(ptv->store, position + offset, item);
    trg_tree_view_select_item(ptv->tv, item);
    g_object_unref(item);
}

static void trg_persistent_tree_view_up_cb(GtkWidget *w, gpointer data)
{
    trg_persistent_tree_view_move(TRG_PERSISTENT_TREE_VIEW(data), -1);
}

static void trg_persistent_tree_view_down_cb(GtkWidget *w, gpointer data)
{
    trg_persistent_tree_view_move(TRG_PERSISTENT_TREE_VIEW(data), 1);
}

static void trg_persistent_tree_view_del_cb(GtkWidget *w, gpointer data)
{
    TrgPersistentTreeView *self = TRG_PERSISTENT_TREE_VIEW(data);
    GList *items = trg_tree_view_get_selected_items(self->tv), *li;
    guint position;

    for (li = items; li; li = g_list_next(li))
        if (g_list_store_find(self->store, li->data, &position))
            g_list_store_remove(self->store, position);

    g_list_free(items);
}

static void trg_persistent_tree_view_finalize(GObject *object)
{
    TrgPersistentTreeView *self = TRG_PERSISTENT_TREE_VIEW(object);
    GSList *li;

    for (li = self->columns; li; li = g_slist_next(li)) {
        trg_persistent_tree_view_column *cd = li->data;
        g_free(cd->key);
        g_free(cd);
    }
    g_slist_free(self->columns);
    g_clear_object(&self->store);

    G_OBJECT_CLASS(trg_persistent_tree_view_parent_class)->finalize(object);
}

trg_pref_widget_desc *trg_persistent_tree_view_get_widget_desc(TrgPersistentTreeView *ptv)
{
    return ptv->wd;
}

void trg_persistent_tree_view_set_add_select(TrgPersistentTreeView *ptv,
                                             trg_persistent_tree_view_column *cd)
{
    ptv->addSelect = cd;
}

static GtkWidget *trg_persistent_tree_view_button_new(TrgPersistentTreeView *ptv, GtkWidget *box,
                                                      const gchar *label, GCallback callback,
                                                      gboolean sensitive)
{
    GtkWidget *w = gtk_button_new_with_label(label);

    gtk_widget_set_sensitive(w, sensitive);
    g_signal_connect(w, "clicked", callback, ptv);
    gtk_widget_set_margin_start(w, 4);
    gtk_widget_set_margin_end(w, 4);
    gtk_box_append(GTK_BOX(box), w);

    return w;
}

static void trg_persistent_tree_view_class_init(TrgPersistentTreeViewClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);

    object_class->finalize = trg_persistent_tree_view_finalize;
}

static void trg_persistent_tree_view_init(TrgPersistentTreeView *ptv)
{
    GtkWidget *hbox;

    gtk_orientable_set_orientation(GTK_ORIENTABLE(ptv), GTK_ORIENTATION_VERTICAL);

    ptv->store = g_list_store_new(TRG_TYPE_PERSISTENT_ITEM);

    /* no prefs: nothing about the view itself is persisted */
    ptv->tv = TRG_TREE_VIEW(trg_tree_view_new());
    trg_tree_view_set_sortable(ptv->tv, FALSE);
    trg_tree_view_set_column_menus(ptv->tv, FALSE);
    trg_tree_view_set_model(ptv->tv, G_LIST_MODEL(ptv->store));
    g_signal_connect(trg_tree_view_get_selection(ptv->tv), "selection-changed",
                     G_CALLBACK(trg_persistent_tree_view_selection_changed), ptv);
    /* removing selected entries doesn't change the selection */
    g_signal_connect_after(trg_tree_view_get_selection(ptv->tv), "items-changed",
                           G_CALLBACK(trg_persistent_tree_view_items_changed), ptv);
    g_signal_connect(ptv->tv, "row-activated", G_CALLBACK(trg_persistent_tree_view_row_activated),
                     ptv);

    gtk_widget_set_vexpand(GTK_WIDGET(ptv->tv), TRUE);
    gtk_widget_set_margin_top(GTK_WIDGET(ptv->tv), 4);
    gtk_widget_set_margin_bottom(GTK_WIDGET(ptv->tv), 4);
    gtk_box_append(GTK_BOX(ptv), GTK_WIDGET(ptv->tv));

    hbox = trg_hbox_new(FALSE, 0);
    trg_persistent_tree_view_button_new(ptv, hbox, _("Add"),
                                        G_CALLBACK(trg_persistent_tree_view_add_cb), TRUE);
    ptv->delButton = trg_persistent_tree_view_button_new(
        ptv, hbox, _("Delete"), G_CALLBACK(trg_persistent_tree_view_del_cb), FALSE);
    ptv->upButton = trg_persistent_tree_view_button_new(
        ptv, hbox, _("Up"), G_CALLBACK(trg_persistent_tree_view_up_cb), FALSE);
    ptv->downButton = trg_persistent_tree_view_button_new(
        ptv, hbox, _("Down"), G_CALLBACK(trg_persistent_tree_view_down_cb), FALSE);
    gtk_widget_set_margin_top(hbox, 4);
    gtk_widget_set_margin_bottom(hbox, 4);
    gtk_box_append(GTK_BOX(ptv), hbox);
}

TrgPersistentTreeView *trg_persistent_tree_view_new(const gchar *key, gint conf_flags)
{
    TrgPersistentTreeView *ptv = g_object_new(TRG_TYPE_PERSISTENT_TREE_VIEW, NULL);

    ptv->wd = trg_pref_widget_desc_new(GTK_WIDGET(ptv), (gchar *)key, conf_flags);
    ptv->wd->saveFunc = &trg_persistent_tree_view_save;
    ptv->wd->refreshFunc = &trg_persistent_tree_view_refresh;

    return ptv;
}
