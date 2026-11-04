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

#include "json.h"
#include "protocol-constants.h"
#include "requests.h"
#include "trg-client.h"
#include "trg-file-rename-dialog.h"
#include "trg-files-model.h"
#include "trg-files-tree-view.h"
#include "trg-main-window.h"
#include "trg-tree-view.h"
#include "util.h"

struct _TrgFilesTreeView {
    TrgTreeView parent;

    TrgFilesModel *model;
    /* NULL for a local view */
    TrgClient *client;
    TrgMainWindow *win;
};

G_DEFINE_TYPE(TrgFilesTreeView, trg_files_tree_view, TRG_TYPE_TREE_VIEW)

TrgFilesModel *trg_files_tree_view_get_files_model(TrgFilesTreeView *tv)
{
    return tv->model;
}

gboolean on_files_update(gpointer data)
{
    trg_response *response = (trg_response *)data;
    TrgFilesTreeView *self = TRG_FILES_TREE_VIEW(response->cb_data);

    trg_files_model_set_accept(self->model, TRUE);

    response->cb_data = self->win;
    on_generic_interactive_action_response(data);

    /* the reference taken when sending */
    g_object_unref(self);

    return FALSE;
}

/* Send the wanted state and priority of all the files */
static void send_updated_file_prefs(TrgFilesTreeView *self)
{
    TrgFilesModel *model = self->model;
    JsonArray *targetIdArray;
    JsonObject *args;
    JsonNode *req;
    guint i, n = trg_files_model_get_n_files(model);

    if (!self->client)
        return;

    targetIdArray = json_array_new();
    json_array_add_int_element(targetIdArray, trg_files_model_get_torrent_id(model));

    req = torrent_set(targetIdArray);
    args = node_get_arguments(req);

    for (i = 0; i < n; i++) {
        TrgFileItem *item = trg_files_model_get_file(model, i);
        gint priority;

        if (!item)
            continue;

        if (trg_item_get_int(item, FILESCOL_WANTED))
            add_file_id_to_array(args, FIELD_FILES_WANTED, i);
        else
            add_file_id_to_array(args, FIELD_FILES_UNWANTED, i);

        priority = trg_item_get_int(item, FILESCOL_PRIORITY);
        if (priority == TR_PRI_LOW)
            add_file_id_to_array(args, FIELD_FILES_PRIORITY_LOW, i);
        else if (priority == TR_PRI_HIGH)
            add_file_id_to_array(args, FIELD_FILES_PRIORITY_HIGH, i);
        else
            add_file_id_to_array(args, FIELD_FILES_PRIORITY_NORMAL, i);
    }

    trg_files_model_set_accept(model, FALSE);

    dispatch_rpc_async(self->client, req, on_files_update, g_object_ref(self));
}

static void files_changed(TrgFilesTreeView *self)
{
    trg_tree_view_resort(TRG_TREE_VIEW(self));
    send_updated_file_prefs(self);
}

/* A click on the Download check box toggles it */
static void wanted_activate(TrgTreeView *tv, gpointer item, gpointer data)
{
    /* a mixed directory is skipped */
    trg_file_item_set_wanted(item, !trg_item_get_int(item, FILESCOL_WANTED));
    files_changed(TRG_FILES_TREE_VIEW(tv));
}

/* A click on the priority goes from normal to high to low */
static void priority_activate(TrgTreeView *tv, gpointer item, gpointer data)
{
    gint priority;

    switch (trg_item_get_int(item, FILESCOL_PRIORITY)) {
    case TR_PRI_NORMAL:
        priority = TR_PRI_HIGH;
        break;
    case TR_PRI_HIGH:
        priority = TR_PRI_LOW;
        break;
    default:
        priority = TR_PRI_NORMAL;
        break;
    }

    trg_file_item_set_priority(item, priority);
    files_changed(TRG_FILES_TREE_VIEW(tv));
}

/***
 * Context menu
 */

static void set_selected(TrgFilesTreeView *self, gint column, gint value)
{
    GList *items = trg_tree_view_get_selected_items(TRG_TREE_VIEW(self));
    GList *li;

    for (li = items; li; li = g_list_next(li)) {
        if (column == FILESCOL_WANTED)
            trg_file_item_set_wanted(li->data, value);
        else
            trg_file_item_set_priority(li->data, value);
    }

    g_list_free(items);

    files_changed(self);
}

static void rename_action_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    TrgFilesTreeView *self = TRG_FILES_TREE_VIEW(data);

    gtk_window_present(GTK_WINDOW(trg_file_rename_dialog_new(self->win, self->client, self)));
}

static void priority_high_action_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    set_selected(TRG_FILES_TREE_VIEW(data), FILESCOL_PRIORITY, TR_PRI_HIGH);
}

static void priority_normal_action_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    set_selected(TRG_FILES_TREE_VIEW(data), FILESCOL_PRIORITY, TR_PRI_NORMAL);
}

static void priority_low_action_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    set_selected(TRG_FILES_TREE_VIEW(data), FILESCOL_PRIORITY, TR_PRI_LOW);
}

static void wanted_action_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    set_selected(TRG_FILES_TREE_VIEW(data), FILESCOL_WANTED, TRUE);
}

static void unwanted_action_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    set_selected(TRG_FILES_TREE_VIEW(data), FILESCOL_WANTED, FALSE);
}

static void expand_all_action_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    trg_tree_view_expand_all(TRG_TREE_VIEW(data));
}

static void collapse_all_action_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    trg_tree_view_collapse_all(TRG_TREE_VIEW(data));
}

static void view_popup_menu(TrgFilesTreeView *self, gdouble x, gdouble y)
{
    static const GActionEntry entries[] = {
        { "rename", rename_action_cb, NULL, NULL, NULL, { 0 } },
        { "priority-high", priority_high_action_cb, NULL, NULL, NULL, { 0 } },
        { "priority-normal", priority_normal_action_cb, NULL, NULL, NULL, { 0 } },
        { "priority-low", priority_low_action_cb, NULL, NULL, NULL, { 0 } },
        { "wanted", wanted_action_cb, NULL, NULL, NULL, { 0 } },
        { "unwanted", unwanted_action_cb, NULL, NULL, NULL, { 0 } },
        { "expand-all", expand_all_action_cb, NULL, NULL, NULL, { 0 } },
        { "collapse-all", collapse_all_action_cb, NULL, NULL, NULL, { 0 } },
    };
    static const gchar *selection_actions[]
        = { "priority-high", "priority-normal", "priority-low", "wanted", "unwanted" };
    guint n_selected = trg_tree_view_count_selected(TRG_TREE_VIEW(self));
    GSimpleActionGroup *actions = g_simple_action_group_new();
    GtkBuilder *builder = gtk_builder_new_from_resource(
        "/io/github/TransmissionRemoteGtk/ui/trg-files-tree-view.ui");
    GMenu *menu = g_menu_new();
    guint i;

    g_action_map_add_action_entries(G_ACTION_MAP(actions), entries, G_N_ELEMENTS(entries), self);

    for (i = 0; i < G_N_ELEMENTS(selection_actions); i++)
        g_simple_action_set_enabled(G_SIMPLE_ACTION(g_action_map_lookup_action(
                                        G_ACTION_MAP(actions), selection_actions[i])),
                                    n_selected > 0);

    /* Files of a torrent on the server can be renamed */
    if (self->client) {
        /* Rename on multiple files currently not supported */
        g_simple_action_set_enabled(
            G_SIMPLE_ACTION(g_action_map_lookup_action(G_ACTION_MAP(actions), "rename")),
            n_selected == 1);
        g_menu_append_section(menu, NULL,
                              G_MENU_MODEL(gtk_builder_get_object(builder, "rename_menu")));
    }

    g_menu_append_section(menu, NULL, G_MENU_MODEL(gtk_builder_get_object(builder, "files_menu")));

    trg_popover_menu_popup(GTK_WIDGET(self), G_MENU_MODEL(menu), x, y, "files",
                           G_ACTION_GROUP(actions));

    g_object_unref(menu);
    g_object_unref(builder);
    g_object_unref(actions);
}

static gboolean view_onContextMenu(GtkWidget *widget, gdouble x, gdouble y, gpointer data)
{
    TrgTreeView *tv = TRG_TREE_VIEW(widget);

    /* from the keyboard */
    if (x < 0) {
        view_popup_menu(TRG_FILES_TREE_VIEW(widget), x, y);
        return TRUE;
    }

    /* the column headers have their own menu */
    if (trg_tree_view_point_in_header(tv, x, y) || !trg_tree_view_select_at(tv, x, y))
        return FALSE;

    view_popup_menu(TRG_FILES_TREE_VIEW(widget), x, y);

    return TRUE;
}

/* Enter or a double click opens or closes a directory */
static void row_activated_cb(TrgTreeView *tv, guint position, gpointer data)
{
    g_autoptr(GObject) row
        = g_list_model_get_item(G_LIST_MODEL(trg_tree_view_get_selection(tv)), position);

    if (GTK_IS_TREE_LIST_ROW(row) && gtk_tree_list_row_is_expandable(GTK_TREE_LIST_ROW(row)))
        gtk_tree_list_row_set_expanded(GTK_TREE_LIST_ROW(row),
                                       !gtk_tree_list_row_get_expanded(GTK_TREE_LIST_ROW(row)));
}

/* A new tree is shown expanded */
static void model_rebuilt_cb(TrgFilesModel *model, TrgTreeView *tv)
{
    trg_tree_view_expand_all(tv);
}

static void trg_files_tree_view_dispose(GObject *object)
{
    g_clear_object(&TRG_FILES_TREE_VIEW(object)->model);

    G_OBJECT_CLASS(trg_files_tree_view_parent_class)->dispose(object);
}

static void trg_files_tree_view_class_init(TrgFilesTreeViewClass *klass)
{
    G_OBJECT_CLASS(klass)->dispose = trg_files_tree_view_dispose;
}

static void trg_files_tree_view_init(TrgFilesTreeView *self)
{
}

static TrgFilesTreeView *trg_files_tree_view_create(TrgFilesModel *model, TrgPrefs *prefs,
                                                    const gchar *configId, gboolean progress)
{
    TrgFilesTreeView *self
        = g_object_new(TRG_TYPE_FILES_TREE_VIEW, "config-id", configId, "prefs", prefs, NULL);
    TrgTreeView *tv = TRG_TREE_VIEW(self);
    trg_column_description *desc;

    self->model = g_object_ref(model);

    desc = trg_tree_view_reg_column(tv, TRG_COLTYPE_FILEICONTEXT, FILESCOL_NAME, _("Name"), "name",
                                    0);
    desc->model_column_extra = FILESCOL_ID;

    trg_tree_view_reg_column(tv, TRG_COLTYPE_SIZE, FILESCOL_SIZE, _("Size"), "size", 0);
    if (progress)
        trg_tree_view_reg_column(tv, TRG_COLTYPE_PROG, FILESCOL_PROGRESS, _("Progress"), "progress",
                                 0);
    desc = trg_tree_view_reg_column(tv, TRG_COLTYPE_WANTED, FILESCOL_WANTED, _("Download"),
                                    "wanted", 0);
    desc->activate = wanted_activate;
    desc = trg_tree_view_reg_column(tv, TRG_COLTYPE_PRIO, FILESCOL_PRIORITY, _("Priority"),
                                    "priority", 0);
    desc->activate = priority_activate;

    trg_tree_view_set_search_column(tv, FILESCOL_NAME);

    trg_tree_view_set_model(tv, trg_files_model_get_tree_model(model));
    trg_tree_view_setup_columns(tv);
    trg_tree_view_restore_sort(tv);

    g_signal_connect_object(model, "rebuilt", G_CALLBACK(model_rebuilt_cb), self, 0);
    g_signal_connect(self, "row-activated", G_CALLBACK(row_activated_cb), NULL);
    trg_widget_add_context_menu(GTK_WIDGET(self), view_onContextMenu, NULL);

    /* the tree may be there already */
    trg_tree_view_expand_all(tv);

    return self;
}

TrgFilesTreeView *trg_files_tree_view_new(TrgFilesModel *model, TrgMainWindow *win,
                                          TrgClient *client, const gchar *configId)
{
    TrgFilesTreeView *self
        = trg_files_tree_view_create(model, trg_client_get_prefs(client), configId, TRUE);

    self->client = client;
    self->win = win;

    return self;
}

TrgFilesTreeView *trg_files_tree_view_new_local(TrgFilesModel *model, TrgPrefs *prefs,
                                                const gchar *configId)
{
    return trg_files_tree_view_create(model, prefs, configId, FALSE);
}
