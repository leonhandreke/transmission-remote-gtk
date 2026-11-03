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

#include <gtk/gtk.h>
#include <json-glib/json-glib.h>
#include <string.h>

#include "protocol-constants.h"
#include "torrent.h"
#include "trg-files-model.h"
#include "trg-files-tree.h"
#include "trg-item.h"
#include "trg-model.h"

/***
 * TrgFileItem
 */

static const TrgItemColumn file_item_columns[] = {
    { "name", G_TYPE_STRING },
    { "size", G_TYPE_INT64 },
    { "progress", G_TYPE_DOUBLE },
    { "id", G_TYPE_INT },
    { "wanted", G_TYPE_INT },
    { "priority", G_TYPE_INT },
    { "bytes-completed", G_TYPE_INT64 },
};

struct _TrgFileItem {
    TrgItem parent;

    GListStore *children;
    /* not a reference, the parent owns its children */
    TrgFileItem *parent_item;
};

G_DEFINE_TYPE(TrgFileItem, trg_file_item, TRG_TYPE_ITEM)

static void trg_file_item_dispose(GObject *object)
{
    g_clear_object(&TRG_FILE_ITEM(object)->children);

    G_OBJECT_CLASS(trg_file_item_parent_class)->dispose(object);
}

static void trg_file_item_class_init(TrgFileItemClass *klass)
{
    G_OBJECT_CLASS(klass)->dispose = trg_file_item_dispose;
    trg_item_class_install_columns(klass, file_item_columns, G_N_ELEMENTS(file_item_columns));
}

static void trg_file_item_init(TrgFileItem *self)
{
}

GListStore *trg_file_item_get_children(TrgFileItem *item)
{
    return item->children;
}

TrgFileItem *trg_file_item_get_parent(TrgFileItem *item)
{
    return item->parent_item;
}

gchar *trg_file_item_get_path(TrgFileItem *item)
{
    GString *path = g_string_new(trg_item_get_string(item, FILESCOL_NAME));
    TrgFileItem *parent;

    for (parent = item->parent_item; parent; parent = parent->parent_item) {
        g_string_prepend_c(path, '/');
        g_string_prepend(path, trg_item_get_string(parent, FILESCOL_NAME));
    }

    return g_string_free(path, FALSE);
}

/* Sum up or combine the columns of a directory's children */
static void trg_file_item_update_dir(TrgFileItem *dir)
{
    GListModel *children = G_LIST_MODEL(dir->children);
    guint i, n = g_list_model_get_n_items(children);
    gint64 size = 0, completed = 0;
    gint wanted = TRUE, priority = TR_PRI_NORMAL;

    for (i = 0; i < n; i++) {
        g_autoptr(TrgItem) child = g_list_model_get_item(children, i);
        gint child_wanted = trg_item_get_int(child, FILESCOL_WANTED);
        gint child_priority = trg_item_get_int(child, FILESCOL_PRIORITY);

        size += trg_item_get_int64(child, FILESCOL_SIZE);
        completed += trg_item_get_int64(child, FILESCOL_BYTESCOMPLETED);

        if (i == 0) {
            wanted = child_wanted;
            priority = child_priority;
        } else {
            if (wanted != child_wanted)
                wanted = TR_PRI_MIXED;
            if (priority != child_priority)
                priority = TR_PRI_MIXED;
        }
    }

    trg_item_set(dir, FILESCOL_SIZE, size, FILESCOL_BYTESCOMPLETED, completed, FILESCOL_PROGRESS,
                 file_get_progress(size, completed), FILESCOL_WANTED, wanted, FILESCOL_PRIORITY,
                 priority, -1);
}

/* Update the directories of a subtree, the deepest first */
static void trg_file_item_update_tree(TrgFileItem *item)
{
    GListModel *children = G_LIST_MODEL(item->children);
    guint i, n;

    if (!children)
        return;

    n = g_list_model_get_n_items(children);
    for (i = 0; i < n; i++) {
        g_autoptr(TrgFileItem) child = g_list_model_get_item(children, i);
        trg_file_item_update_tree(child);
    }

    trg_file_item_update_dir(item);
}

static void trg_file_item_set_subtree(TrgFileItem *item, gint column, gint value)
{
    if (item->children) {
        GListModel *children = G_LIST_MODEL(item->children);
        guint i, n = g_list_model_get_n_items(children);

        for (i = 0; i < n; i++) {
            g_autoptr(TrgFileItem) child = g_list_model_get_item(children, i);
            trg_file_item_set_subtree(child, column, value);
        }
    }

    trg_item_set(item, column, value, -1);
}

static void trg_file_item_set_column(TrgFileItem *item, gint column, gint value)
{
    TrgFileItem *parent;

    trg_file_item_set_subtree(item, column, value);

    for (parent = item->parent_item; parent; parent = parent->parent_item)
        trg_file_item_update_dir(parent);
}

void trg_file_item_set_wanted(TrgFileItem *item, gint wanted)
{
    trg_file_item_set_column(item, FILESCOL_WANTED, wanted);
}

void trg_file_item_set_priority(TrgFileItem *item, gint priority)
{
    trg_file_item_set_column(item, FILESCOL_PRIORITY, priority);
}

/***
 * TrgFilesModel
 */

enum {
    SIGNAL_REBUILT,
    SIGNAL_COUNT
};

static guint signals[SIGNAL_COUNT] = { 0 };

struct _TrgFilesModel {
    TrgListModel parent;

    gint64 torrentId;
    gboolean accept;
    /* the file items by file index (not references) */
    GPtrArray *files;
    GListModel *tree_model;

    /* a tree being built in a thread */
    guint build_id;
    gboolean building;
};

G_DEFINE_TYPE(TrgFilesModel, trg_files_model, TRG_TYPE_LIST_MODEL)

static TrgFileItem *trg_files_model_item_new(TrgFilesModel *model, TrgFileItem *parent,
                                             trg_files_tree_node *node, gboolean defaults)
{
    TrgFileItem *item = g_object_new(TRG_TYPE_FILE_ITEM, NULL);
    GList *li;

    item->parent_item = parent;

    trg_item_set(item, FILESCOL_NAME, node->name, FILESCOL_ID, node->children ? -1 : node->index,
                 -1);

    if (node->children) {
        GPtrArray *children = g_ptr_array_new_with_free_func(g_object_unref);

        for (li = node->children; li; li = g_list_next(li))
            g_ptr_array_add(
                children,
                trg_files_model_item_new(model, item, (trg_files_tree_node *)li->data, defaults));

        item->children = g_list_store_new(TRG_TYPE_FILE_ITEM);
        g_list_store_splice(item->children, 0, 0, children->pdata, children->len);
        g_ptr_array_unref(children);
    } else {
        if (defaults)
            trg_item_set(item, FILESCOL_SIZE, node->length, FILESCOL_WANTED, TRUE,
                         FILESCOL_PRIORITY, TR_PRI_NORMAL, -1);
        else
            trg_item_set(item, FILESCOL_SIZE, node->length, FILESCOL_BYTESCOMPLETED,
                         node->bytesCompleted, FILESCOL_PROGRESS,
                         file_get_progress(node->length, node->bytesCompleted), FILESCOL_WANTED,
                         node->enabled, FILESCOL_PRIORITY, node->priority, -1);

        if (node->index >= 0) {
            if ((guint)node->index >= model->files->len)
                g_ptr_array_set_size(model->files, node->index + 1);
            model->files->pdata[node->index] = item;
        }
    }

    return item;
}

static void trg_files_model_remove_all(TrgFilesModel *model)
{
    model->build_id++;
    model->building = FALSE;
    g_ptr_array_set_size(model->files, 0);
    g_list_store_remove_all(trg_list_model_get_store(TRG_LIST_MODEL(model)));
}

void trg_files_model_clear(TrgFilesModel *model)
{
    trg_files_model_remove_all(model);
    model->torrentId = -1;
}

/* Replace the items with the tree under top (a nameless node, or a single
 * file) */
static void trg_files_model_set_nodes(TrgFilesModel *model, trg_files_tree_node *top,
                                      gboolean defaults)
{
    GPtrArray *items = g_ptr_array_new_with_free_func(g_object_unref);
    GList *li;
    guint i;

    trg_files_model_remove_all(model);

    if (top->name) {
        g_ptr_array_add(items, trg_files_model_item_new(model, NULL, top, defaults));
    } else {
        for (li = top->children; li; li = g_list_next(li))
            g_ptr_array_add(
                items,
                trg_files_model_item_new(model, NULL, (trg_files_tree_node *)li->data, defaults));
    }

    for (i = 0; i < items->len; i++)
        trg_file_item_update_tree(items->pdata[i]);

    /* add the top level in one go */
    g_list_store_splice(trg_list_model_get_store(TRG_LIST_MODEL(model)), 0, 0, items->pdata,
                        items->len);
    g_ptr_array_unref(items);

    g_signal_emit(model, signals[SIGNAL_REBUILT], 0);
}

guint trg_files_model_set_tree(TrgFilesModel *model, trg_files_tree_node *top)
{
    guint i, n = 0;

    trg_files_model_set_nodes(model, top, TRUE);
    model->accept = TRUE;

    for (i = 0; i < model->files->len; i++)
        if (model->files->pdata[i])
            n++;

    return n;
}

void trg_files_model_set_accept(TrgFilesModel *model, gboolean accept)
{
    model->accept = accept;
}

guint trg_files_model_get_n_files(TrgFilesModel *model)
{
    return model->files->len;
}

TrgFileItem *trg_files_model_get_file(TrgFilesModel *model, guint index)
{
    return index < model->files->len ? model->files->pdata[index] : NULL;
}

static GListModel *trg_files_model_create_children(gpointer item, gpointer data)
{
    GListStore *children = trg_file_item_get_children(TRG_FILE_ITEM(item));

    return children ? g_object_ref(G_LIST_MODEL(children)) : NULL;
}

GListModel *trg_files_model_get_tree_model(TrgFilesModel *model)
{
    /* of the store, as the model holds the tree model */
    if (!model->tree_model)
        model->tree_model = G_LIST_MODEL(gtk_tree_list_model_new(
            G_LIST_MODEL(g_object_ref(trg_list_model_get_store(TRG_LIST_MODEL(model)))), FALSE,
            FALSE, trg_files_model_create_children, NULL, NULL));

    return model->tree_model;
}

static void trg_files_model_item_update(TrgFilesModel *self, TrgFileItem *item, JsonObject *file,
                                        JsonArray *wantedArray, JsonArray *prioritiesArray, gint id)
{
    gint64 fileLength = file_get_length(file);
    gint64 fileCompleted = file_get_bytes_completed(file);

    trg_item_set(item, FILESCOL_SIZE, fileLength, FILESCOL_PROGRESS,
                 file_get_progress(fileLength, fileCompleted), FILESCOL_BYTESCOMPLETED,
                 fileCompleted, -1);

    if (self->accept) {
        gchar **path = g_strsplit(file_get_name(file), "/", -1);
        guint depth = g_strv_length(path);
        TrgFileItem *parent;

        trg_item_set(item, FILESCOL_WANTED, (gint)json_array_get_int_element(wantedArray, id),
                     FILESCOL_PRIORITY, (gint)json_array_get_int_element(prioritiesArray, id), -1);

        /* the names (of the file and its directories) change on a rename */
        if (depth > 0) {
            trg_item_set(item, FILESCOL_NAME, path[--depth], -1);
            for (parent = item->parent_item; parent && depth > 0; parent = parent->parent_item)
                trg_item_set(parent, FILESCOL_NAME, path[--depth], -1);
        }

        g_strfreev(path);
    }
}

/* Builds a tree of the files of a torrent with simple data structures,
 * which is quicker than going straight to the model, and can be done in a
 * thread. */
static trg_files_tree_node *trg_files_model_node_insert(trg_files_tree_node *top,
                                                        trg_files_tree_node *last, JsonObject *file,
                                                        gint index, JsonArray *enabled,
                                                        JsonArray *priorities)
{
    gchar **path = g_strsplit(file_get_name(file), "/", -1);
    trg_files_tree_node *lastIter = last;
    GList *parentList = NULL;
    gchar *path_el;
    GList *li;
    int i;

    /* Build up a list of pointers to each parent trg_files_tree_node
     * reversing the order as it iterates over its parent.
     */
    if (lastIter)
        while ((lastIter = lastIter->parent))
            parentList = g_list_prepend(parentList, lastIter);

    li = parentList;
    lastIter = top;

    /* Iterate over the path list which contains each file/directory
     * component of the path in order.
     */
    for (i = 0; (path_el = path[i]); i++) {
        gboolean isFile = !path[i + 1];
        trg_files_tree_node *target_node = NULL;

        /* No point checking for files. If there is a last parents iterator
         * check it for a shortcut. I'm assuming that these come in order of
         * directory at least to give performance a boost.
         */
        if (li && !isFile) {
            trg_files_tree_node *lastPathNode = (trg_files_tree_node *)li->data;

            if (!g_strcmp0(lastPathNode->name, path[i])) {
                target_node = lastPathNode;
                li = g_list_next(li);
            } else {
                /* No need to check any further. */
                li = NULL;
            }
        }

        if (!target_node && lastIter && lastIter->childrenHash && !isFile)
            target_node = g_hash_table_lookup(lastIter->childrenHash, path_el);

        /* Node needs creating */

        if (!target_node) {
            target_node = g_new0(trg_files_tree_node, 1);
            target_node->name = g_strdup(path[i]);
            target_node->parent = lastIter;
            trg_files_tree_node_add_child(lastIter, target_node);
        }

        lastIter = target_node;

        /* The directories are summed up from their files in the model */
        if (isFile) {
            target_node->length = file_get_length(file);
            target_node->bytesCompleted = file_get_bytes_completed(file);
            target_node->index = index;
            target_node->enabled = (gint)json_array_get_int_element(enabled, index);
            target_node->priority = (gint)json_array_get_int_element(priorities, index);
        } else {
            target_node->index = -1;
        }
    }

    g_list_free(parentList);
    g_strfreev(path);

    return lastIter;
}

struct FirstUpdateThreadData {
    TrgFilesModel *model;
    guint build_id;
    JsonArray *files;
    JsonArray *priorities;
    JsonArray *wanted;
    trg_files_tree_node *top_node;
    gboolean idle_add;
};

static gboolean trg_files_model_applytree_idlefunc(gpointer data)
{
    struct FirstUpdateThreadData *args = (struct FirstUpdateThreadData *)data;
    TrgFilesModel *self = args->model;

    /* unless another torrent was selected or the model cleared since */
    if (args->build_id == self->build_id) {
        trg_files_model_set_nodes(self, args->top_node, FALSE);
        self->accept = TRUE;
    }

    trg_files_tree_node_free(args->top_node);
    json_array_unref(args->files);
    json_array_unref(args->priorities);
    json_array_unref(args->wanted);
    g_object_unref(args->model);
    g_free(data);

    return G_SOURCE_REMOVE;
}

static gpointer trg_files_model_buildtree_threadfunc(gpointer data)
{
    struct FirstUpdateThreadData *args = (struct FirstUpdateThreadData *)data;
    trg_files_tree_node *lastNode = NULL;
    guint i, n = json_array_get_length(args->files);

    args->top_node = g_new0(trg_files_tree_node, 1);

    for (i = 0; i < n; i++)
        lastNode = trg_files_model_node_insert(args->top_node, lastNode,
                                               json_array_get_object_element(args->files, i), i,
                                               args->wanted, args->priorities);

    if (args->idle_add)
        g_idle_add(trg_files_model_applytree_idlefunc, data);

    return NULL;
}

void trg_files_model_update(TrgFilesModel *model, gint64 updateSerial G_GNUC_UNUSED, JsonObject *t,
                            gint mode)
{
    JsonArray *files = torrent_get_files(t);
    guint n_files = json_array_get_length(files);
    JsonArray *priorities = torrent_get_priorities(t);
    JsonArray *wanted = torrent_get_wanted(t);
    gint64 torrentId = torrent_get_id(t);

    if (mode == TORRENT_GET_MODE_FIRST || torrentId != model->torrentId
        || model->files->len != n_files) {
        struct FirstUpdateThreadData *futd;

        /* the tree of this torrent is on its way */
        if (mode != TORRENT_GET_MODE_FIRST && torrentId == model->torrentId && model->building)
            return;

        trg_files_model_remove_all(model);
        model->torrentId = torrentId;

        futd = g_new0(struct FirstUpdateThreadData, 1);
        futd->model = g_object_ref(model);
        futd->build_id = model->build_id;
        futd->files = json_array_ref(files);
        futd->priorities = json_array_ref(priorities);
        futd->wanted = json_array_ref(wanted);
        futd->idle_add = n_files > TRG_FILES_MODEL_CREATE_THREAD_IF_GT;

        /* If this update has more than a given number of files, build up the
         * simple tree in a thread, then g_idle_add a function which
         * adds the contents of this prebuilt tree.
         *
         * If less than or equal to, I don't think it's worth spawning threads
         * for. Just do it in the main loop.
         */
        if (futd->idle_add) {
            model->building = TRUE;
            g_thread_unref(g_thread_new(NULL, trg_files_model_buildtree_threadfunc, futd));
        } else {
            trg_files_model_buildtree_threadfunc(futd);
            trg_files_model_applytree_idlefunc(futd);
        }
    } else {
        GListModel *store = G_LIST_MODEL(trg_list_model_get_store(TRG_LIST_MODEL(model)));
        guint i, n;

        for (i = 0; i < n_files; i++) {
            TrgFileItem *item = model->files->pdata[i];
            if (item)
                trg_files_model_item_update(model, item, json_array_get_object_element(files, i),
                                            wanted, priorities, i);
        }

        n = g_list_model_get_n_items(store);
        for (i = 0; i < n; i++) {
            g_autoptr(TrgFileItem) item = g_list_model_get_item(store, i);
            trg_file_item_update_tree(item);
        }
    }
}

gint64 trg_files_model_get_torrent_id(TrgFilesModel *model)
{
    return model->torrentId;
}

static void trg_files_model_dispose(GObject *object)
{
    TrgFilesModel *self = TRG_FILES_MODEL(object);

    g_clear_object(&self->tree_model);

    G_OBJECT_CLASS(trg_files_model_parent_class)->dispose(object);
}

static void trg_files_model_finalize(GObject *object)
{
    TrgFilesModel *self = TRG_FILES_MODEL(object);

    g_ptr_array_unref(self->files);

    G_OBJECT_CLASS(trg_files_model_parent_class)->finalize(object);
}

static void trg_files_model_class_init(TrgFilesModelClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);

    object_class->dispose = trg_files_model_dispose;
    object_class->finalize = trg_files_model_finalize;

    signals[SIGNAL_REBUILT] = g_signal_new("rebuilt", G_TYPE_FROM_CLASS(object_class),
                                           G_SIGNAL_RUN_LAST, 0, NULL, NULL, NULL, G_TYPE_NONE, 0);
}

static void trg_files_model_init(TrgFilesModel *self)
{
    self->torrentId = -1;
    self->accept = TRUE;
    self->files = g_ptr_array_new();
}

TrgFilesModel *trg_files_model_new(void)
{
    return g_object_new(TRG_TYPE_FILES_MODEL, "item-type", TRG_TYPE_FILE_ITEM, NULL);
}
