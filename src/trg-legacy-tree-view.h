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

#include "trg-prefs.h"

#define TRG_TYPE_LEGACY_TREE_VIEW trg_legacy_tree_view_get_type()
G_DECLARE_DERIVABLE_TYPE(TrgLegacyTreeView, trg_legacy_tree_view, TRG, LEGACY_TREE_VIEW,
                         GtkTreeView)

struct _TrgLegacyTreeViewClass {
    GtkTreeViewClass parent_class;
};

GtkWidget *trg_legacy_tree_view_new(void);

GList *trg_legacy_tree_view_get_selected_refs_list(GtkTreeView *tv);

typedef enum {
    TRG_LEGACY_COLTYPE_ICONTEXT,
    TRG_LEGACY_COLTYPE_FILEICONTEXT,
    TRG_LEGACY_COLTYPE_WANTED,
    TRG_LEGACY_COLTYPE_TEXT,
    TRG_LEGACY_COLTYPE_SIZE,
    TRG_LEGACY_COLTYPE_RATIO,
    TRG_LEGACY_COLTYPE_EPOCH,
    TRG_LEGACY_COLTYPE_SPEED,
    TRG_LEGACY_COLTYPE_ETA,
    TRG_LEGACY_COLTYPE_PROG,
    TRG_LEGACY_COLTYPE_PRIO,
    TRG_LEGACY_COLTYPE_NUMGTZERO,
    TRG_LEGACY_COLTYPE_NUMGTEQZERO
} TrgLegacyColumnType;

typedef struct {
    gint model_column;
    gint model_column_extra;
    gchar *header;
    gchar *id;
    guint flags;
    guint type;
    GtkCellRenderer *customRenderer;
    GtkTreeViewColumn **out;
} trg_legacy_column_description;

#define TRG_LEGACY_COLUMN_DEFAULT            0x00
#define TRG_LEGACY_COLUMN_SHOWING            (1 << 0) /* 0x01 */
#define TRG_LEGACY_COLUMN_UNREMOVABLE        (1 << 1) /* 0x02 */
#define TRG_LEGACY_COLUMN_EXTRA              (1 << 2) /* 0x04 */
#define TRG_LEGACY_COLUMN_HIDE_FROM_TOP_MENU (1 << 3) /* 0x08 */

#define TRG_LEGACY_TREE_VIEW_PERSIST_SORT   (1 << 0)
#define TRG_LEGACY_TREE_VIEW_PERSIST_LAYOUT (1 << 1)

trg_legacy_column_description *trg_legacy_tree_view_reg_column(TrgLegacyTreeView *tv, gint type,
                                                               gint model_column,
                                                               const gchar *header, const gchar *id,
                                                               guint flags);
void trg_legacy_tree_view_setup_columns(TrgLegacyTreeView *tv);
void trg_legacy_tree_view_set_prefs(TrgLegacyTreeView *tv, TrgPrefs *prefs);
void trg_legacy_tree_view_persist(TrgLegacyTreeView *tv, guint flags);
gboolean trg_legacy_tree_view_is_column_showing(TrgLegacyTreeView *tv, gint index);
