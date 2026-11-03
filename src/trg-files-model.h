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

#include <gio/gio.h>
#include <glib-object.h>
#include <json-glib/json-glib.h>

#include "trg-files-tree.h"
#include "trg-item.h"
#include "trg-model.h"

G_BEGIN_DECLS

enum {
    FILESCOL_NAME,
    FILESCOL_SIZE,
    FILESCOL_PROGRESS,
    FILESCOL_ID,
    FILESCOL_WANTED,
    FILESCOL_PRIORITY,
    FILESCOL_BYTESCOMPLETED,
    FILESCOL_COLUMNS
};

/* A file, or a directory (ID -1) with its children in a GListStore. The
 * columns of a directory sum up (size, bytes completed, progress) or combine
 * (wanted, priority: TR_PRI_MIXED if its files differ) those of its
 * children. */
#define TRG_TYPE_FILE_ITEM trg_file_item_get_type()
G_DECLARE_FINAL_TYPE(TrgFileItem, trg_file_item, TRG, FILE_ITEM, TrgItem)

/* NULL for a file */
GListStore *trg_file_item_get_children(TrgFileItem *item);
/* NULL at the top level */
TrgFileItem *trg_file_item_get_parent(TrgFileItem *item);
/* The path within the torrent, "dir/sub/name" */
gchar *trg_file_item_get_path(TrgFileItem *item);

/* Set the wanted (TRUE/FALSE) or priority (TR_PRI_*) of the item and all
 * its descendants, and update its ancestors to match. */
void trg_file_item_set_wanted(TrgFileItem *item, gint wanted);
void trg_file_item_set_priority(TrgFileItem *item, gint priority);

/* The tree of the files of a torrent. As a GListModel, it's the top level
 * items; trg_files_model_get_tree_model() gives the whole tree for a
 * TrgTreeView. Emits "rebuilt" when it was filled with a new tree. */
#define TRG_TYPE_FILES_MODEL trg_files_model_get_type()
G_DECLARE_FINAL_TYPE(TrgFilesModel, trg_files_model, TRG, FILES_MODEL, TrgListModel)

TrgFilesModel *trg_files_model_new(void);

#define TRG_FILES_MODEL_CREATE_THREAD_IF_GT 600

/* From a torrent's JSON: builds the tree on the first update or when the
 * number of files changed (in a thread for many files), otherwise updates
 * the items in place. */
void trg_files_model_update(TrgFilesModel *model, gint64 updateSerial, JsonObject *t, gint mode);
gint64 trg_files_model_get_torrent_id(TrgFilesModel *model);
/* While FALSE, updates don't overwrite the names, wanted and priorities
 * (a change of the user's is on its way to the daemon). */
void trg_files_model_set_accept(TrgFilesModel *model, gboolean accept);
void trg_files_model_clear(TrgFilesModel *model);

/* Fills the model from a parsed tree (trg_parse_torrent_file()), all files
 * wanted with normal priority. Returns the number of files. */
guint trg_files_model_set_tree(TrgFilesModel *model, trg_files_tree_node *top);

/* A GtkTreeListModel of the tree, owned by the model */
GListModel *trg_files_model_get_tree_model(TrgFilesModel *model);

/* The files (not directories) by their index; slots may be NULL */
guint trg_files_model_get_n_files(TrgFilesModel *model);
TrgFileItem *trg_files_model_get_file(TrgFilesModel *model, guint index);

G_END_DECLS
