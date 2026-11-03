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

G_BEGIN_DECLS

/* A list or tree of TrgItems in a GtkColumnView (in its own scrolled
 * window), with registered columns the user can show, hide, reorder, resize
 * and sort by, which are persisted in TrgPrefs. */
#define TRG_TYPE_TREE_VIEW trg_tree_view_get_type()
G_DECLARE_DERIVABLE_TYPE(TrgTreeView, trg_tree_view, TRG, TREE_VIEW, GtkWidget)

struct _TrgTreeViewClass {
    GtkWidgetClass parent_class;
};

typedef enum {
    TRG_COLTYPE_ICONTEXT,
    TRG_COLTYPE_FILEICONTEXT,
    TRG_COLTYPE_WANTED,
    TRG_COLTYPE_TEXT,
    TRG_COLTYPE_SIZE,
    TRG_COLTYPE_RATIO,
    TRG_COLTYPE_EPOCH,
    TRG_COLTYPE_SPEED,
    TRG_COLTYPE_ETA,
    TRG_COLTYPE_PROG,
    TRG_COLTYPE_PRIO,
    TRG_COLTYPE_NUMGTZERO,
    TRG_COLTYPE_NUMGTEQZERO
} TrgColumnType;

typedef struct _trg_column_description trg_column_description;

/* Called when the user toggles a "wanted" cell or clicks a priority cell */
typedef void (*TrgColumnActivateFunc)(TrgTreeView *tv, gpointer item, gpointer data);

/* Called when the user starts editing an editable text cell (new_text and
 * done are FALSE/NULL), and when they finish (done is TRUE, new_text is NULL
 * if the text wasn't changed). */
typedef void (*TrgColumnEditFunc)(TrgTreeView *tv, gpointer item, gboolean done,
                                  const gchar *new_text, gpointer data);

struct _trg_column_description {
    gint model_column;
    gint model_column_extra;
    gchar *header;
    gchar *id;
    guint flags;
    guint type;

    TrgColumnActivateFunc activate;
    TrgColumnEditFunc edit;
    gpointer func_data;
    gboolean editable;

    /* while showing */
    GtkColumnViewColumn *column;
    TrgTreeView *tv;
};

#define TRG_COLUMN_DEFAULT            0x00
#define TRG_COLUMN_SHOWING            (1 << 0) /* 0x01 */
#define TRG_COLUMN_UNREMOVABLE        (1 << 1) /* 0x02 */
#define TRG_COLUMN_EXTRA              (1 << 2) /* 0x04 */
#define TRG_COLUMN_HIDE_FROM_TOP_MENU (1 << 3) /* 0x08 */
#define TRG_COLUMN_EDITABLE           (1 << 4) /* 0x10 */

#define TRG_TREE_VIEW_PERSIST_SORT   (1 << 0)
#define TRG_TREE_VIEW_PERSIST_LAYOUT (1 << 1)

GtkWidget *trg_tree_view_new(void);

trg_column_description *trg_tree_view_reg_column(TrgTreeView *tv, gint type, gint model_column,
                                                 const gchar *header, const gchar *id, guint flags);
void trg_tree_view_setup_columns(TrgTreeView *tv);
void trg_tree_view_remove_all_columns(TrgTreeView *tv);
void trg_tree_view_set_prefs(TrgTreeView *tv, TrgPrefs *prefs);
void trg_tree_view_persist(TrgTreeView *tv, guint flags);
void trg_tree_view_restore_sort(TrgTreeView *tv);
gboolean trg_tree_view_is_column_showing(TrgTreeView *tv, gint index);
void trg_tree_view_set_column_editable(trg_column_description *desc, gboolean editable);
void trg_tree_view_start_editing(TrgTreeView *tv, gpointer item, trg_column_description *desc);
void trg_tree_view_stop_editing(TrgTreeView *tv);

/* The model is a GListModel of TrgItems, or a GtkTreeListModel of them. */
void trg_tree_view_set_model(TrgTreeView *tv, GListModel *model);
GListModel *trg_tree_view_get_model(TrgTreeView *tv);
void trg_tree_view_set_sortable(TrgTreeView *tv, gboolean sortable);
void trg_tree_view_set_search_column(TrgTreeView *tv, gint column);
void trg_tree_view_set_column_menus(TrgTreeView *tv, gboolean enabled);

/* Shows the rows with factory in a GtkListView instead of the columns, or
 * the columns again when factory is NULL. */
void trg_tree_view_set_list_factory(TrgTreeView *tv, GtkListItemFactory *factory);

/* Sorting by a model column, -1 for the model's order */
void trg_tree_view_set_sort(TrgTreeView *tv, gint column, GtkSortType order);
gint trg_tree_view_get_sort(TrgTreeView *tv, GtkSortType *order);
void trg_tree_view_resort(TrgTreeView *tv);

/* Selection. Items are TrgItems (the items of the rows of a tree). */
GtkSelectionModel *trg_tree_view_get_selection(TrgTreeView *tv);
GList *trg_tree_view_get_selected_items(TrgTreeView *tv);
guint trg_tree_view_count_selected(TrgTreeView *tv);
gpointer trg_tree_view_get_first_selected(TrgTreeView *tv);
gpointer trg_tree_view_get_item_at(TrgTreeView *tv, gdouble x, gdouble y, guint *position);
gboolean trg_tree_view_select_at(TrgTreeView *tv, gdouble x, gdouble y);
gboolean trg_tree_view_point_in_header(TrgTreeView *tv, gdouble x, gdouble y);
void trg_tree_view_select_item(TrgTreeView *tv, gpointer item);
gboolean trg_tree_view_get_focus_rect(TrgTreeView *tv, graphene_rect_t *rect);
gpointer trg_tree_view_row_item(gpointer row);

/* For row widgets of a list factory, so their rows can be found */
void trg_tree_view_mark_row_widget(GtkWidget *widget, GtkListItem *list_item);

/* Expanding the rows of a tree */
void trg_tree_view_expand_all(TrgTreeView *tv);
void trg_tree_view_collapse_all(TrgTreeView *tv);

#define TRG_TREE_VIEW_SORT_TYPE_ACTION   "sort-type"
#define TRG_TREE_VIEW_SORT_COLUMN_ACTION "sort-column"

/* Creates the stateful sort actions in map (a window, so "win." actions) and
 * returns a menu using them. */
GMenu *trg_tree_view_sort_menu(TrgTreeView *tv, GActionMap *map);

G_END_DECLS
