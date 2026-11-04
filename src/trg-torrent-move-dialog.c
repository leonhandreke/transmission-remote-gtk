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
#include "torrent.h"
#include "trg-client.h"
#include "trg-destination-combo.h"
#include "trg-main-window.h"
#include "trg-torrent-move-dialog.h"
#include "util.h"

struct _TrgTorrentMoveDialog {
    TrgDialog parent;

    TrgClient *client;
    TrgMainWindow *win;
    TrgTorrentTreeView *treeview;
    JsonArray *ids;
    GtkWidget *grid;
    GtkWidget *location_label;
    GtkWidget *location_combo;
    GtkWidget *move_check;
};

G_DEFINE_TYPE(TrgTorrentMoveDialog, trg_torrent_move_dialog, TRG_TYPE_DIALOG)

enum {
    PROP_0,
    PROP_CLIENT,
    PROP_PARENT_WINDOW,
    PROP_TREEVIEW
};

static void trg_torrent_move_response_cb(TrgDialog *dialog, gint res_id, gpointer data)
{
    TrgTorrentMoveDialog *dlg = TRG_TORRENT_MOVE_DIALOG(dialog);

    if (res_id == GTK_RESPONSE_ACCEPT) {
        gchar *location = trg_destination_combo_get_dir(TRG_DESTINATION_COMBO(dlg->location_combo));
        JsonNode *request = torrent_set_location(
            dlg->ids, location, gtk_check_button_get_active(GTK_CHECK_BUTTON(dlg->move_check)));
        g_free(location);
        trg_destination_combo_save_selection(TRG_DESTINATION_COMBO(dlg->location_combo));
        dispatch_rpc_async(dlg->client, request, on_generic_interactive_action_response, data);
    } else {
        json_array_unref(dlg->ids);
    }
    gtk_window_destroy(GTK_WINDOW(dlg));
}

static void location_changed(TrgDestinationCombo *w, gpointer data)
{
    TrgTorrentMoveDialog *dlg = TRG_TORRENT_MOVE_DIALOG(data);
    gtk_widget_action_set_enabled(
        GTK_WIDGET(dlg), "dialog.accept",
        trg_destination_combo_has_text(TRG_DESTINATION_COMBO(dlg->location_combo)));
}

static void trg_torrent_move_dialog_constructed(GObject *object)
{
    TrgTorrentMoveDialog *self = TRG_TORRENT_MOVE_DIALOG(object);
    gint count;
    gchar *msg;

    G_OBJECT_CLASS(trg_torrent_move_dialog_parent_class)->constructed(object);

    self->location_combo
        = trg_destination_combo_new(self->client, TRG_PREFS_KEY_LAST_MOVE_DESTINATION);
    gtk_widget_set_hexpand(self->location_combo, TRUE);
    g_signal_connect(self->location_combo, "changed", G_CALLBACK(location_changed), object);
    gtk_grid_attach(GTK_GRID(self->grid), self->location_combo, 1, 0, 1, 1);
    gtk_label_set_mnemonic_widget(GTK_LABEL(self->location_label), self->location_combo);
    location_changed(TRG_DESTINATION_COMBO(self->location_combo), self);

    count = trg_tree_view_count_selected(TRG_TREE_VIEW(self->treeview));
    self->ids = build_json_id_array(self->treeview);

    if (count == 1) {
        JsonObject *json;
        const gchar *name;

        get_torrent_data(trg_client_get_torrent_table(self->client),
                         trg_mw_get_selected_torrent_id(self->win), &json, NULL);
        name = torrent_get_name(json);
        msg = g_strdup_printf(_("Move %s"), name);
    } else {
        msg = g_strdup_printf(_("Move %d torrents"), count);
    }

    gtk_window_set_transient_for(GTK_WINDOW(object), GTK_WINDOW(self->win));
    gtk_window_set_title(GTK_WINDOW(object), msg);

    g_signal_connect(G_OBJECT(object), "response", G_CALLBACK(trg_torrent_move_response_cb),
                     self->win);

    g_free(msg);
}

static void trg_torrent_move_dialog_dispose(GObject *object)
{
    gtk_widget_dispose_template(GTK_WIDGET(object), TRG_TYPE_TORRENT_MOVE_DIALOG);

    G_OBJECT_CLASS(trg_torrent_move_dialog_parent_class)->dispose(object);
}

static void trg_torrent_move_dialog_get_property(GObject *object, guint property_id, GValue *value,
                                                 GParamSpec *pspec)
{
    TrgTorrentMoveDialog *self = TRG_TORRENT_MOVE_DIALOG(object);
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

static void trg_torrent_move_dialog_set_property(GObject *object, guint property_id,
                                                 const GValue *value, GParamSpec *pspec)
{
    TrgTorrentMoveDialog *self = TRG_TORRENT_MOVE_DIALOG(object);
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

static void trg_torrent_move_dialog_class_init(TrgTorrentMoveDialogClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);
    GtkWidgetClass *widget_class = GTK_WIDGET_CLASS(klass);

    object_class->get_property = trg_torrent_move_dialog_get_property;
    object_class->set_property = trg_torrent_move_dialog_set_property;
    object_class->constructed = trg_torrent_move_dialog_constructed;
    object_class->dispose = trg_torrent_move_dialog_dispose;

    gtk_widget_class_set_template_from_resource(
        widget_class, "/io/github/TransmissionRemoteGtk/ui/trg-torrent-move-dialog.ui");
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentMoveDialog, grid);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentMoveDialog, location_label);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentMoveDialog, move_check);

    g_object_class_install_property(
        object_class, PROP_TREEVIEW,
        g_param_spec_object("torrent-tree-view", "TrgTorrentTreeView", "TrgTorrentTreeView",
                            TRG_TYPE_TORRENT_TREE_VIEW,
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

static void trg_torrent_move_dialog_init(TrgTorrentMoveDialog *self)
{
    gtk_widget_init_template(GTK_WIDGET(self));
}

TrgTorrentMoveDialog *trg_torrent_move_dialog_new(TrgMainWindow *win, TrgClient *client,
                                                  TrgTorrentTreeView *ttv)
{
    return g_object_new(TRG_TYPE_TORRENT_MOVE_DIALOG, "trg-client", client, "torrent-tree-view",
                        ttv, "parent-window", win, NULL);
}
