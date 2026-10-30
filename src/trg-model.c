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

#include "trg-item.h"
#include "trg-model.h"

typedef struct {
    GListStore *store;
    GType item_type;
} TrgListModelPrivate;

enum {
    PROP_0,
    PROP_ITEM_TYPE
};

static void trg_list_model_list_model_init(GListModelInterface *iface);

G_DEFINE_TYPE_WITH_CODE(TrgListModel, trg_list_model, G_TYPE_OBJECT,
                        G_ADD_PRIVATE(TrgListModel)
                            G_IMPLEMENT_INTERFACE(G_TYPE_LIST_MODEL,
                                                  trg_list_model_list_model_init))

GListStore *trg_list_model_get_store(TrgListModel *model)
{
    TrgListModelPrivate *priv = trg_list_model_get_instance_private(model);

    return priv->store;
}

static GType trg_list_model_get_item_type(GListModel *list)
{
    TrgListModelPrivate *priv = trg_list_model_get_instance_private(TRG_LIST_MODEL(list));

    return priv->item_type;
}

static guint trg_list_model_get_n_items(GListModel *list)
{
    return g_list_model_get_n_items(G_LIST_MODEL(trg_list_model_get_store(TRG_LIST_MODEL(list))));
}

static gpointer trg_list_model_get_item(GListModel *list, guint position)
{
    return g_list_model_get_item(G_LIST_MODEL(trg_list_model_get_store(TRG_LIST_MODEL(list))),
                                 position);
}

static void trg_list_model_list_model_init(GListModelInterface *iface)
{
    iface->get_item_type = trg_list_model_get_item_type;
    iface->get_n_items = trg_list_model_get_n_items;
    iface->get_item = trg_list_model_get_item;
}

static void trg_list_model_store_changed(GListModel *store, guint position, guint removed,
                                         guint added, TrgListModel *self)
{
    g_list_model_items_changed(G_LIST_MODEL(self), position, removed, added);
}

static void trg_list_model_set_property(GObject *object, guint property_id, const GValue *value,
                                        GParamSpec *pspec)
{
    TrgListModelPrivate *priv = trg_list_model_get_instance_private(TRG_LIST_MODEL(object));

    switch (property_id) {
    case PROP_ITEM_TYPE:
        priv->item_type = g_value_get_gtype(value);
        break;
    default:
        G_OBJECT_WARN_INVALID_PROPERTY_ID(object, property_id, pspec);
        break;
    }
}

static void trg_list_model_constructed(GObject *object)
{
    TrgListModel *self = TRG_LIST_MODEL(object);
    TrgListModelPrivate *priv = trg_list_model_get_instance_private(self);

    G_OBJECT_CLASS(trg_list_model_parent_class)->constructed(object);

    priv->store = g_list_store_new(priv->item_type);
    g_signal_connect(priv->store, "items-changed", G_CALLBACK(trg_list_model_store_changed), self);
}

static void trg_list_model_dispose(GObject *object)
{
    TrgListModelPrivate *priv = trg_list_model_get_instance_private(TRG_LIST_MODEL(object));

    if (priv->store) {
        g_signal_handlers_disconnect_by_data(priv->store, object);
        g_clear_object(&priv->store);
    }

    G_OBJECT_CLASS(trg_list_model_parent_class)->dispose(object);
}

static void trg_list_model_class_init(TrgListModelClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);

    object_class->set_property = trg_list_model_set_property;
    object_class->constructed = trg_list_model_constructed;
    object_class->dispose = trg_list_model_dispose;

    g_object_class_install_property(
        object_class, PROP_ITEM_TYPE,
        g_param_spec_gtype("item-type", NULL, NULL, TRG_TYPE_ITEM,
                           G_PARAM_WRITABLE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_STRINGS));
}

static void trg_list_model_init(TrgListModel *self)
{
}

/* Removes the items whose serial column isn't currentSerial, in runs, so
 * it's linear in the number of items. Returns how many were removed. */
guint trg_model_remove_removed(GListStore *store, gint serial_column, gint64 currentSerial)
{
    GListModel *model = G_LIST_MODEL(store);
    guint n = g_list_model_get_n_items(model);
    guint removed = 0;
    guint i = n;

    while (i > 0) {
        guint end = i;

        /* find a run of stale items ending at end - 1 */
        while (i > 0) {
            g_autoptr(TrgItem) item = g_list_model_get_item(model, i - 1);
            if (trg_item_get_int64(item, serial_column) == currentSerial)
                break;
            i--;
        }

        if (i < end) {
            g_list_store_splice(store, i, end - i, NULL, 0);
            removed += end - i;
        } else {
            i--;
        }
    }

    return removed;
}

/* The first item whose search_column (an integer column) is id */
gpointer trg_model_find_item(GListModel *model, gint search_column, gint64 id, guint *position)
{
    guint i, n = g_list_model_get_n_items(model);

    for (i = 0; i < n; i++) {
        g_autoptr(TrgItem) item = g_list_model_get_item(model, i);
        GValue value = G_VALUE_INIT;
        gint64 current;

        g_value_init(&value, G_TYPE_INT64);
        g_value_transform(trg_item_peek_value(item, search_column), &value);
        current = g_value_get_int64(&value);

        if (current == id) {
            if (position)
                *position = i;
            return item;
        }
    }

    return NULL;
}
