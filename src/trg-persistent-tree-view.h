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
#include <gtk/gtk.h>

#include "trg-preferences-dialog.h"

/* An editable list of string records (JSON objects with a string member per
 * column) stored as a JSON array in TrgPrefs, with buttons to add, delete and
 * move entries. The list is saved and refreshed through its widget desc. */
#define TRG_TYPE_PERSISTENT_TREE_VIEW trg_persistent_tree_view_get_type()
G_DECLARE_FINAL_TYPE(TrgPersistentTreeView, trg_persistent_tree_view, TRG, PERSISTENT_TREE_VIEW,
                     GtkBox)

#define TRG_PERSISTENT_TREE_VIEW_MAX_COLUMNS 2

typedef struct _trg_persistent_tree_view_column trg_persistent_tree_view_column;

TrgPersistentTreeView *trg_persistent_tree_view_new(const gchar *key, gint conf_flags);

trg_pref_widget_desc *trg_persistent_tree_view_get_widget_desc(TrgPersistentTreeView *ptv);

/* The column that is edited when an entry is added */
void trg_persistent_tree_view_set_add_select(TrgPersistentTreeView *ptv,
                                             trg_persistent_tree_view_column *cd);

/* index < TRG_PERSISTENT_TREE_VIEW_MAX_COLUMNS, key is the JSON member */
trg_persistent_tree_view_column *trg_persistent_tree_view_add_column(TrgPersistentTreeView *ptv,
                                                                     gint index, const gchar *key,
                                                                     const gchar *label);
