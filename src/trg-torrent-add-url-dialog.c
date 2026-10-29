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

#include "hig.h"
#include "requests.h"
#include "trg-client.h"
#include "trg-main-window.h"
#include "trg-torrent-add-url-dialog.h"
#include "util.h"

struct _TrgTorrentAddUrlDialog {
    GtkDialog parent;

    TrgClient *client;
    TrgMainWindow *win;
    GtkWidget *urlEntry;
    GtkWidget *startCheck;
    GtkWidget *addButton;
};

G_DEFINE_TYPE(TrgTorrentAddUrlDialog, trg_torrent_add_url_dialog, GTK_TYPE_DIALOG)

static void trg_torrent_add_url_dialog_class_init(TrgTorrentAddUrlDialogClass *klass)
{
}

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
    GtkWidget *dialog = gtk_message_dialog_new(GTK_WINDOW(dlg->win), GTK_DIALOG_MODAL,
                                               GTK_MESSAGE_WARNING, GTK_BUTTONS_OK, "%s", msg);
    gtk_window_set_title(GTK_WINDOW(dialog), _("Error"));
    g_signal_connect(dialog, "response", G_CALLBACK(gtk_window_destroy), NULL);
    gtk_window_present(GTK_WINDOW(dialog));
}

static void trg_torrent_add_url_response_cb(TrgTorrentAddUrlDialog *dlg, gint res_id, gpointer data)
{
    if (res_id == GTK_RESPONSE_ACCEPT) {
        JsonNode *request;
        const gchar *entryText = gtk_editable_get_text(GTK_EDITABLE(dlg->urlEntry));

        if (g_str_has_prefix(entryText, "magnet:") && !has_dht_support(dlg))
            show_dht_not_enabled_warning(dlg);

        request = torrent_add_url(entryText,
                                  gtk_check_button_get_active(GTK_CHECK_BUTTON(dlg->startCheck)));
        dispatch_rpc_async(dlg->client, request, on_generic_interactive_action_response, data);
    }

    gtk_window_destroy(GTK_WINDOW(dlg));
}

static void url_entry_changed(GtkWidget *w, gpointer data)
{
    TrgTorrentAddUrlDialog *dlg = TRG_TORRENT_ADD_URL_DIALOG(data);
    gtk_widget_set_sensitive(dlg->addButton, gtk_entry_get_text_length(GTK_ENTRY(w)) > 0);
}

static void url_entry_activate(GtkWidget *w, gpointer data)
{
    gtk_dialog_response(GTK_DIALOG(data), GTK_RESPONSE_ACCEPT);
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

static void trg_torrent_add_url_dialog_init(TrgTorrentAddUrlDialog *self)
{
    GtkWidget *w, *t, *contentvbox;
    guint row = 0;

    contentvbox = gtk_dialog_get_content_area(GTK_DIALOG(self));

    t = hig_workarea_create();

    w = self->urlEntry = gtk_entry_new();
    g_signal_connect(w, "changed", G_CALLBACK(url_entry_changed), self);
    g_signal_connect(w, "activate", G_CALLBACK(url_entry_activate), self);
    gdk_clipboard_read_text_async(gtk_widget_get_clipboard(w), NULL, clipboard_text_received,
                                  g_object_ref(w));

    hig_workarea_add_row(t, &row, _("URL:"), w, NULL);

    self->startCheck = hig_workarea_add_wide_checkbutton(t, &row, _("Start Paused"), FALSE);

    gtk_window_set_title(GTK_WINDOW(self), _("Add torrent from URL"));
    gtk_window_set_destroy_with_parent(GTK_WINDOW(self), TRUE);

    gtk_dialog_add_button(GTK_DIALOG(self), _("_Close"), GTK_RESPONSE_CANCEL);
    self->addButton = gtk_dialog_add_button(GTK_DIALOG(self), _("_Add"), GTK_RESPONSE_ACCEPT);
    gtk_widget_set_sensitive(self->addButton, FALSE);

    trg_widget_set_margins(gtk_dialog_get_content_area(GTK_DIALOG(self)), GUI_PAD);

    gtk_dialog_set_default_response(GTK_DIALOG(self), GTK_RESPONSE_ACCEPT);

    trg_widget_set_margins(t, GUI_PAD);

    gtk_widget_set_vexpand(t, TRUE);
    gtk_box_append(GTK_BOX(contentvbox), t);
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
