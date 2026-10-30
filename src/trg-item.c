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

#include <gobject/gvaluecollector.h>

#include "trg-item.h"

typedef struct {
    GValue *values;
} TrgItemPrivate;

G_DEFINE_TYPE_WITH_PRIVATE(TrgItem, trg_item, G_TYPE_OBJECT)

static GValue *trg_item_value(TrgItem *item, gint column)
{
    TrgItemPrivate *priv = trg_item_get_instance_private(item);
    TrgItemClass *klass = TRG_ITEM_GET_CLASS(item);

    g_return_val_if_fail(column >= 0 && (guint)column < klass->n_columns, NULL);

    return &priv->values[column];
}

guint trg_item_get_n_columns(gpointer item)
{
    return TRG_ITEM_GET_CLASS(item)->n_columns;
}

GParamSpec *trg_item_get_column_pspec(gpointer item, gint column)
{
    TrgItemClass *klass = TRG_ITEM_GET_CLASS(item);

    g_return_val_if_fail(column >= 0 && (guint)column < klass->n_columns, NULL);

    return klass->columns[column];
}

GType trg_item_get_column_type(gpointer item, gint column)
{
    return G_PARAM_SPEC_VALUE_TYPE(trg_item_get_column_pspec(item, column));
}

static gboolean trg_item_values_equal(const GValue *a, const GValue *b)
{
    switch (G_TYPE_FUNDAMENTAL(G_VALUE_TYPE(a))) {
    case G_TYPE_STRING:
        return !g_strcmp0(g_value_get_string(a), g_value_get_string(b));
    case G_TYPE_INT:
        return g_value_get_int(a) == g_value_get_int(b);
    case G_TYPE_UINT:
        return g_value_get_uint(a) == g_value_get_uint(b);
    case G_TYPE_INT64:
        return g_value_get_int64(a) == g_value_get_int64(b);
    case G_TYPE_DOUBLE:
        return g_value_get_double(a) == g_value_get_double(b);
    case G_TYPE_BOOLEAN:
        return g_value_get_boolean(a) == g_value_get_boolean(b);
    case G_TYPE_POINTER:
        return g_value_get_pointer(a) == g_value_get_pointer(b);
    case G_TYPE_BOXED:
        return g_value_get_boxed(a) == g_value_get_boxed(b);
    default:
        return FALSE;
    }
}

void trg_item_set_value(gpointer item, gint column, const GValue *value)
{
    GValue *current = trg_item_value(TRG_ITEM(item), column);

    if (!current || trg_item_values_equal(current, value))
        return;

    g_value_copy(value, g_value_reset(current));
    g_object_notify_by_pspec(G_OBJECT(item), trg_item_get_column_pspec(item, column));
}

void trg_item_set(gpointer item, ...)
{
    va_list args;
    gint column;

    g_object_freeze_notify(G_OBJECT(item));

    va_start(args, item);
    while ((column = va_arg(args, gint)) >= 0) {
        GValue value = G_VALUE_INIT;
        gchar *error = NULL;

        G_VALUE_COLLECT_INIT(&value, trg_item_get_column_type(item, column), args, 0, &error);
        if (error) {
            g_critical("%s: %s", G_STRFUNC, error);
            g_free(error);
            break;
        }

        trg_item_set_value(item, column, &value);
        g_value_unset(&value);
    }
    va_end(args);

    g_object_thaw_notify(G_OBJECT(item));
}

void trg_item_get(gpointer item, ...)
{
    va_list args;
    gint column;

    va_start(args, item);
    while ((column = va_arg(args, gint)) >= 0) {
        GValue *value = trg_item_value(TRG_ITEM(item), column);
        gchar *error = NULL;

        G_VALUE_LCOPY(value, args, 0, &error);
        if (error) {
            g_critical("%s: %s", G_STRFUNC, error);
            g_free(error);
            break;
        }
    }
    va_end(args);
}

const GValue *trg_item_peek_value(gpointer item, gint column)
{
    return trg_item_value(TRG_ITEM(item), column);
}

const gchar *trg_item_get_string(gpointer item, gint column)
{
    return g_value_get_string(trg_item_value(TRG_ITEM(item), column));
}

gint64 trg_item_get_int64(gpointer item, gint column)
{
    return g_value_get_int64(trg_item_value(TRG_ITEM(item), column));
}

gint trg_item_get_int(gpointer item, gint column)
{
    return g_value_get_int(trg_item_value(TRG_ITEM(item), column));
}

guint trg_item_get_uint(gpointer item, gint column)
{
    return g_value_get_uint(trg_item_value(TRG_ITEM(item), column));
}

gdouble trg_item_get_double(gpointer item, gint column)
{
    return g_value_get_double(trg_item_value(TRG_ITEM(item), column));
}

gboolean trg_item_get_boolean(gpointer item, gint column)
{
    return g_value_get_boolean(trg_item_value(TRG_ITEM(item), column));
}

gpointer trg_item_get_pointer(gpointer item, gint column)
{
    return g_value_get_pointer(trg_item_value(TRG_ITEM(item), column));
}

gpointer trg_item_get_boxed(gpointer item, gint column)
{
    return g_value_get_boxed(trg_item_value(TRG_ITEM(item), column));
}

#define TRG_CMP(a, b) ((a) < (b) ? -1 : ((a) > (b) ? 1 : 0))

gint trg_item_compare(gpointer a, gpointer b, gint column)
{
    const GValue *va = trg_item_value(TRG_ITEM(a), column);
    const GValue *vb = trg_item_value(TRG_ITEM(b), column);

    switch (G_TYPE_FUNDAMENTAL(G_VALUE_TYPE(va))) {
    case G_TYPE_STRING: {
        const gchar *sa = g_value_get_string(va);
        const gchar *sb = g_value_get_string(vb);
        if (!sa || !sb)
            return sa ? 1 : (sb ? -1 : 0);
        return g_utf8_collate(sa, sb);
    }
    case G_TYPE_INT:
        return TRG_CMP(g_value_get_int(va), g_value_get_int(vb));
    case G_TYPE_UINT:
        return TRG_CMP(g_value_get_uint(va), g_value_get_uint(vb));
    case G_TYPE_INT64:
        return TRG_CMP(g_value_get_int64(va), g_value_get_int64(vb));
    case G_TYPE_DOUBLE:
        return TRG_CMP(g_value_get_double(va), g_value_get_double(vb));
    case G_TYPE_BOOLEAN:
        return TRG_CMP(g_value_get_boolean(va), g_value_get_boolean(vb));
    default:
        return 0;
    }
}

static void trg_item_get_property(GObject *object, guint property_id, GValue *value,
                                  GParamSpec *pspec)
{
    g_value_copy(trg_item_value(TRG_ITEM(object), property_id - 1), value);
}

static void trg_item_set_property(GObject *object, guint property_id, const GValue *value,
                                  GParamSpec *pspec)
{
    trg_item_set_value(object, property_id - 1, value);
}

static GParamSpec *trg_item_column_pspec_new(const TrgItemColumn *column)
{
    const GParamFlags flags = G_PARAM_READWRITE | G_PARAM_EXPLICIT_NOTIFY | G_PARAM_STATIC_STRINGS;

    switch (G_TYPE_FUNDAMENTAL(column->type)) {
    case G_TYPE_STRING:
        return g_param_spec_string(column->name, NULL, NULL, NULL, flags);
    case G_TYPE_INT:
        return g_param_spec_int(column->name, NULL, NULL, G_MININT, G_MAXINT, 0, flags);
    case G_TYPE_UINT:
        return g_param_spec_uint(column->name, NULL, NULL, 0, G_MAXUINT, 0, flags);
    case G_TYPE_INT64:
        return g_param_spec_int64(column->name, NULL, NULL, G_MININT64, G_MAXINT64, 0, flags);
    case G_TYPE_DOUBLE:
        return g_param_spec_double(column->name, NULL, NULL, -G_MAXDOUBLE, G_MAXDOUBLE, 0, flags);
    case G_TYPE_BOOLEAN:
        return g_param_spec_boolean(column->name, NULL, NULL, FALSE, flags);
    case G_TYPE_POINTER:
        return g_param_spec_pointer(column->name, NULL, NULL, flags);
    case G_TYPE_BOXED:
        return g_param_spec_boxed(column->name, NULL, NULL, column->type, flags);
    default:
        g_assert_not_reached();
    }
}

void trg_item_class_install_columns(gpointer klass, const TrgItemColumn *columns, guint n_columns)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);
    TrgItemClass *item_class = TRG_ITEM_CLASS(klass);
    guint i;

    object_class->get_property = trg_item_get_property;
    object_class->set_property = trg_item_set_property;

    item_class->n_columns = n_columns;
    item_class->columns = g_new0(GParamSpec *, n_columns + 1);

    for (i = 0; i < n_columns; i++) {
        item_class->columns[i] = trg_item_column_pspec_new(&columns[i]);
        g_object_class_install_property(object_class, i + 1, item_class->columns[i]);
    }
}

static void trg_item_constructed(GObject *object)
{
    TrgItem *self = TRG_ITEM(object);
    TrgItemPrivate *priv = trg_item_get_instance_private(self);
    TrgItemClass *klass = TRG_ITEM_GET_CLASS(self);
    guint i;

    priv->values = g_new0(GValue, klass->n_columns);
    for (i = 0; i < klass->n_columns; i++)
        g_param_value_set_default(
            klass->columns[i],
            g_value_init(&priv->values[i], G_PARAM_SPEC_VALUE_TYPE(klass->columns[i])));

    G_OBJECT_CLASS(trg_item_parent_class)->constructed(object);
}

static void trg_item_finalize(GObject *object)
{
    TrgItem *self = TRG_ITEM(object);
    TrgItemPrivate *priv = trg_item_get_instance_private(self);
    guint i;

    for (i = 0; priv->values && i < TRG_ITEM_GET_CLASS(self)->n_columns; i++)
        g_value_unset(&priv->values[i]);
    g_free(priv->values);

    G_OBJECT_CLASS(trg_item_parent_class)->finalize(object);
}

static void trg_item_class_init(TrgItemClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);

    object_class->constructed = trg_item_constructed;
    object_class->finalize = trg_item_finalize;
}

static void trg_item_init(TrgItem *self)
{
}
