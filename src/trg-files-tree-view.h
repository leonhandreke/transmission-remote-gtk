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

#include "trg-client.h"
#include "trg-files-model.h"
#include "trg-main-window.h"
#include "trg-prefs.h"
#include "trg-tree-view.h"

G_BEGIN_DECLS

/* The files of a torrent as a tree, where the user can choose which files
 * to download and their priorities (for the selected rows from the context
 * menu, or a row by clicking its Download or Priority cell). */
#define TRG_TYPE_FILES_TREE_VIEW trg_files_tree_view_get_type()
G_DECLARE_FINAL_TYPE(TrgFilesTreeView, trg_files_tree_view, TRG, FILES_TREE_VIEW, TrgTreeView)

/* Of a torrent: changes are sent to the daemon, files can be renamed */
TrgFilesTreeView *trg_files_tree_view_new(TrgFilesModel *model, TrgMainWindow *win,
                                          TrgClient *client, const gchar *configId);

/* Only changes the model, eg. of a torrent about to be added (fill it with
 * trg_files_model_set_tree()). No progress column, no renaming. */
TrgFilesTreeView *trg_files_tree_view_new_local(TrgFilesModel *model, TrgPrefs *prefs,
                                                const gchar *configId);

TrgFilesModel *trg_files_tree_view_get_files_model(TrgFilesTreeView *tv);

/* The response to a change sent by the view (or the rename dialog), with
 * the view as the callback data */
gboolean on_files_update(gpointer data);

G_END_DECLS
