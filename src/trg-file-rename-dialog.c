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
#include <json-glib/json-glib.h>
#include <string.h>

#include "requests.h"
#include "trg-client.h"
#include "trg-file-rename-dialog.h"
#include "trg-files-model.h"
#include "trg-files-tree-view.h"
#include "trg-main-window.h"
#include "util.h"

struct _TrgFileRenameDialog {
    TrgDialog parent;

    TrgClient *client;
    TrgMainWindow *win;
    TrgFilesTreeView *treeview;
    JsonArray *ids;
    gchar *orig_path;
    GtkWidget *name_entry;
};

typedef struct {
    TrgDialogClass parent_class;
} _TrgFileRenameDialogClass;

G_DEFINE_TYPE(TrgFileRenameDialog, trg_file_rename_dialog, TRG_TYPE_DIALOG)

enum {
    PROP_0,
    PROP_CLIENT,
    PROP_PARENT_WINDOW,
    PROP_TREEVIEW
};

static void trg_file_rename_response_cb(TrgDialog *dlg, gint res_id, gpointer data)
{
    TrgFileRenameDialog *self = TRG_FILE_RENAME_DIALOG(dlg);

    if (res_id == GTK_RESPONSE_ACCEPT) {
        const gchar *name = gtk_editable_get_text(GTK_EDITABLE(self->name_entry));
        JsonNode *request = torrent_rename_path(self->ids, self->orig_path, name);

        trg_files_model_set_accept(trg_files_tree_view_get_files_model(self->treeview), FALSE);

        dispatch_rpc_async(self->client, request, on_files_update, g_object_ref(self->treeview));
    } else {
        json_array_unref(self->ids);
    }
    g_clear_pointer(&self->orig_path, g_free);
    gtk_window_destroy(GTK_WINDOW(dlg));
}

static void name_changed(GtkEditable *editable, TrgFileRenameDialog *self)
{
    gtk_widget_action_set_enabled(GTK_WIDGET(self), "dialog.accept",
                                  gtk_entry_get_text_length(GTK_ENTRY(editable)) > 0);
}

static void trg_file_rename_dialog_constructed(GObject *object)
{
    TrgFileRenameDialog *self = TRG_FILE_RENAME_DIALOG(object);
    TrgTreeView *tv = TRG_TREE_VIEW(self->treeview);
    TrgFileItem *item = trg_tree_view_get_first_selected(tv);
    guint count = trg_tree_view_count_selected(tv);
    gint64 target_id;
    gchar *orig_filename;
    gchar *msg;

    G_OBJECT_CLASS(trg_file_rename_dialog_parent_class)->constructed(object);

    /* Get file selection info */

    if (item) {
        orig_filename = g_strdup(trg_item_get_string(item, FILESCOL_NAME));
        self->orig_path = trg_file_item_get_path(item);
    } else {
        orig_filename = g_strdup("error");
        self->orig_path = NULL;
    }

    gtk_editable_set_text(GTK_EDITABLE(self->name_entry), orig_filename);

    target_id = trg_files_model_get_torrent_id(trg_files_tree_view_get_files_model(self->treeview));
    self->ids = json_array_new();
    json_array_add_int_element(self->ids, target_id);

    if (count == 1) {
        msg = g_strdup_printf(_("Rename %s"), orig_filename);
    } else {
        /* this really shouldn't happen since TrgFilesTreeView checks number of
         * selected files before allowing rename */
        msg = g_strdup_printf(_("INTERNAL ERROR: Rename %d files not supported"), count);
        gtk_widget_action_set_enabled(GTK_WIDGET(self), "dialog.accept", FALSE);
        gtk_widget_set_sensitive(self->name_entry, FALSE);
    }

    gtk_window_set_transient_for(GTK_WINDOW(object), GTK_WINDOW(self->win));
    gtk_window_set_title(GTK_WINDOW(object), msg);

    g_signal_connect(G_OBJECT(object), "response", G_CALLBACK(trg_file_rename_response_cb),
                     self->win);

    g_free(msg);
    g_free(orig_filename);
}

static void trg_file_rename_dialog_dispose(GObject *object)
{
    gtk_widget_dispose_template(GTK_WIDGET(object), TRG_TYPE_FILE_RENAME_DIALOG);

    G_OBJECT_CLASS(trg_file_rename_dialog_parent_class)->dispose(object);
}

static void trg_file_rename_dialog_get_property(GObject *object, guint property_id, GValue *value,
                                                GParamSpec *pspec)
{
    TrgFileRenameDialog *self = TRG_FILE_RENAME_DIALOG(object);
    switch (property_id) {
    case PROP_CLIENT:
        g_value_set_pointer(value, self->client);
        break;
    case PROP_PARENT_WINDOW:
        g_value_set_object(value, self->win);
        break;
    case PROP_TREEVIEW:
        g_value_set_object(value, self->treeview);
        break;
    default:
        G_OBJECT_WARN_INVALID_PROPERTY_ID(object, property_id, pspec);
        break;
    }
}

static void trg_file_rename_dialog_set_property(GObject *object, guint property_id,
                                                const GValue *value, GParamSpec *pspec)
{
    TrgFileRenameDialog *self = TRG_FILE_RENAME_DIALOG(object);
    switch (property_id) {
    case PROP_CLIENT:
        self->client = g_value_get_pointer(value);
        break;
    case PROP_PARENT_WINDOW:
        self->win = g_value_get_object(value);
        break;
    case PROP_TREEVIEW:
        self->treeview = g_value_get_object(value);
        break;
    default:
        G_OBJECT_WARN_INVALID_PROPERTY_ID(object, property_id, pspec);
        break;
    }
}

static void trg_file_rename_dialog_class_init(TrgFileRenameDialogClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);
    GtkWidgetClass *widget_class = GTK_WIDGET_CLASS(klass);

    object_class->get_property = trg_file_rename_dialog_get_property;
    object_class->set_property = trg_file_rename_dialog_set_property;
    object_class->constructed = trg_file_rename_dialog_constructed;
    object_class->dispose = trg_file_rename_dialog_dispose;

    gtk_widget_class_set_template_from_resource(
        widget_class, "/io/github/TransmissionRemoteGtk/ui/trg-file-rename-dialog.ui");
    gtk_widget_class_bind_template_child(widget_class, TrgFileRenameDialog, name_entry);
    gtk_widget_class_bind_template_callback(widget_class, name_changed);

    g_object_class_install_property(
        object_class, PROP_TREEVIEW,
        g_param_spec_object("files-tree-view", "TrgFilesTreeView", "TrgFilesTreeView",
                            TRG_TYPE_FILES_TREE_VIEW,
                            G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    g_object_class_install_property(
        object_class, PROP_PARENT_WINDOW,
        g_param_spec_object("parent-window", "Parent window", "Parent window", TRG_TYPE_MAIN_WINDOW,
                            G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    g_object_class_install_property(
        object_class, PROP_CLIENT,
        g_param_spec_pointer("trg-client", "TClient", "Client",
                             G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                 | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));
}

static void trg_file_rename_dialog_init(TrgFileRenameDialog *self)
{
    gtk_widget_init_template(GTK_WIDGET(self));
}

TrgFileRenameDialog *trg_file_rename_dialog_new(TrgMainWindow *win, TrgClient *client,
                                                TrgFilesTreeView *ttv)
{
    return g_object_new(TRG_TYPE_FILE_RENAME_DIALOG, "trg-client", client, "files-tree-view", ttv,
                        "parent-window", win, NULL);
}
