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

#include "requests.h"
#include "trg-client.h"
#include "trg-main-window.h"
#include "trg-torrent-add-url-dialog.h"
#include "util.h"

struct _TrgTorrentAddUrlDialog {
    TrgDialog parent;

    TrgClient *client;
    TrgMainWindow *win;
    GtkWidget *url_entry;
    GtkWidget *start_check;
};

G_DEFINE_TYPE(TrgTorrentAddUrlDialog, trg_torrent_add_url_dialog, TRG_TYPE_DIALOG)

static gboolean has_dht_support(TrgTorrentAddUrlDialog *dlg)
{
    JsonObject *session = trg_client_get_session(dlg->client);
    return session_get_dht_enabled(session);
}

static void show_dht_not_enabled_warning(TrgTorrentAddUrlDialog *dlg)
{
    gchar *msg = _("You are trying to add a magnet torrent, but DHT is disabled. Distributed Hash "
                   "Table (DHT) should be enabled in remote settings.");
    /* The add dialog is destroyed right after this, so use the main window as
     * parent now that the warning doesn't block. */
    trg_error_dialog(GTK_WINDOW(dlg->win), msg);
}

static void trg_torrent_add_url_response_cb(TrgTorrentAddUrlDialog *dlg, gint res_id, gpointer data)
{
    if (res_id == GTK_RESPONSE_ACCEPT) {
        JsonNode *request;
        const gchar *entryText = gtk_editable_get_text(GTK_EDITABLE(dlg->url_entry));

        if (g_str_has_prefix(entryText, "magnet:") && !has_dht_support(dlg))
            show_dht_not_enabled_warning(dlg);

        request = torrent_add_url(entryText,
                                  gtk_check_button_get_active(GTK_CHECK_BUTTON(dlg->start_check)));
        dispatch_rpc_async(dlg->client, request, on_generic_interactive_action_response, data);
    }

    gtk_window_destroy(GTK_WINDOW(dlg));
}

static void url_entry_changed(GtkEditable *editable, TrgTorrentAddUrlDialog *self)
{
    gtk_widget_action_set_enabled(GTK_WIDGET(self), "dialog.accept",
                                  gtk_entry_get_text_length(GTK_ENTRY(editable)) > 0);
}

static void clipboard_text_received(GObject *source, GAsyncResult *res, gpointer text_widget)
{
    g_autofree char *text = gdk_clipboard_read_text_finish(GDK_CLIPBOARD(source), res, NULL);

    if (text && g_regex_match_simple("^(https?|magnet):", text, 0, 0)) {
        GtkEditable *e = GTK_EDITABLE(text_widget);
        if (g_strcmp0("", gtk_editable_get_text(e)) == 0) {
            gtk_editable_set_text(e, text);
        }
    }

    g_object_unref(text_widget);
}

static void trg_torrent_add_url_dialog_dispose(GObject *object)
{
    gtk_widget_dispose_template(GTK_WIDGET(object), TRG_TYPE_TORRENT_ADD_URL_DIALOG);

    G_OBJECT_CLASS(trg_torrent_add_url_dialog_parent_class)->dispose(object);
}

static void trg_torrent_add_url_dialog_class_init(TrgTorrentAddUrlDialogClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);
    GtkWidgetClass *widget_class = GTK_WIDGET_CLASS(klass);

    object_class->dispose = trg_torrent_add_url_dialog_dispose;

    gtk_widget_class_set_template_from_resource(
        widget_class, "/io/github/TransmissionRemoteGtk/ui/trg-torrent-add-url-dialog.ui");
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentAddUrlDialog, url_entry);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentAddUrlDialog, start_check);
    gtk_widget_class_bind_template_callback(widget_class, url_entry_changed);
}

static void trg_torrent_add_url_dialog_init(TrgTorrentAddUrlDialog *self)
{
    gtk_widget_init_template(GTK_WIDGET(self));

    gtk_widget_action_set_enabled(GTK_WIDGET(self), "dialog.accept", FALSE);

    gdk_clipboard_read_text_async(gtk_widget_get_clipboard(self->url_entry), NULL,
                                  clipboard_text_received, g_object_ref(self->url_entry));
}

TrgTorrentAddUrlDialog *trg_torrent_add_url_dialog_new(TrgMainWindow *win, TrgClient *client)
{
    GObject *obj = g_object_new(TRG_TYPE_TORRENT_ADD_URL_DIALOG, NULL);
    TrgTorrentAddUrlDialog *self = TRG_TORRENT_ADD_URL_DIALOG(obj);

    self->client = client;
    self->win = win;

    gtk_window_set_transient_for(GTK_WINDOW(obj), GTK_WINDOW(win));
    g_signal_connect(G_OBJECT(obj), "response", G_CALLBACK(trg_torrent_add_url_response_cb), win);

    return TRG_TORRENT_ADD_URL_DIALOG(obj);
}
