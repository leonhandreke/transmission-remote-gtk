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
#pragma once

#include <glib-object.h>

G_BEGIN_DECLS

/* The base class for the items (rows) of the list models. An item has a
 * number of numbered columns, which are also its GObject properties (with
 * property ids column + 1), so they can be bound to widgets and notify when
 * they change. Setting a column to the value it already has doesn't notify.
 *
 * A subclass describes its columns with trg_item_class_install_columns() in
 * its class_init. */
#define TRG_TYPE_ITEM trg_item_get_type()
G_DECLARE_DERIVABLE_TYPE(TrgItem, trg_item, TRG, ITEM, GObject)

struct _TrgItemClass {
    GObjectClass parent_class;

    guint n_columns;
    GParamSpec **columns;
};

typedef struct {
    const gchar *name;
    GType type;
} TrgItemColumn;

void trg_item_class_install_columns(gpointer klass, const TrgItemColumn *columns, guint n_columns);

guint trg_item_get_n_columns(gpointer item);
GParamSpec *trg_item_get_column_pspec(gpointer item, gint column);
GType trg_item_get_column_type(gpointer item, gint column);

/* Like gtk_list_store_set(): column, value pairs terminated by -1 */
void trg_item_set(gpointer item, ...);
void trg_item_set_value(gpointer item, gint column, const GValue *value);

/* Like gtk_tree_model_get(): column, return location pairs terminated by -1.
 * Strings are copied. */
void trg_item_get(gpointer item, ...);
const GValue *trg_item_peek_value(gpointer item, gint column);

const gchar *trg_item_get_string(gpointer item, gint column);
gint64 trg_item_get_int64(gpointer item, gint column);
gint trg_item_get_int(gpointer item, gint column);
guint trg_item_get_uint(gpointer item, gint column);
gdouble trg_item_get_double(gpointer item, gint column);
gboolean trg_item_get_boolean(gpointer item, gint column);
gpointer trg_item_get_pointer(gpointer item, gint column);
gpointer trg_item_get_boxed(gpointer item, gint column);

/* Compares the values of a column, strings by their collation */
gint trg_item_compare(gpointer a, gpointer b, gint column);

/* Defines a final TrgItem subclass (for use in a .c file) with the columns of
 * the TrgItemColumn array columns. */
#define TRG_DEFINE_ITEM_TYPE(TypeName, type_name, MODULE, OBJ_NAME, columns)                       \
    G_DECLARE_FINAL_TYPE(TypeName, type_name, MODULE, OBJ_NAME, TrgItem)                           \
    struct _##TypeName {                                                                           \
        TrgItem parent;                                                                            \
    };                                                                                             \
    G_DEFINE_TYPE(TypeName, type_name, TRG_TYPE_ITEM)                                              \
    static void type_name##_class_init(TypeName##Class *klass)                                     \
    {                                                                                              \
        trg_item_class_install_columns(klass, columns, G_N_ELEMENTS(columns));                     \
    }                                                                                              \
    static void type_name##_init(TypeName *self)                                                   \
    {                                                                                              \
    }

G_END_DECLS
