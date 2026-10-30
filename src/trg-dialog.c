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

#include "hig.h"
#include "trg-dialog.h"

#define RESPONSE_ID_KEY "trg-dialog-response-id"

enum {
    SIGNAL_RESPONSE,
    SIGNAL_COUNT
};

static guint signals[SIGNAL_COUNT] = { 0 };

typedef struct {
    GtkWidget *content_area;
    GtkWidget *action_area;
    gboolean destroyed;
} TrgDialogPrivate;

G_DEFINE_TYPE_WITH_PRIVATE(TrgDialog, trg_dialog, GTK_TYPE_WINDOW)

GtkWidget *trg_dialog_get_content_area(TrgDialog *dialog)
{
    TrgDialogPrivate *priv = trg_dialog_get_instance_private(dialog);

    return priv->content_area;
}

void trg_dialog_response(TrgDialog *dialog, gint response_id)
{
    g_signal_emit(dialog, signals[SIGNAL_RESPONSE], 0, response_id);
}

static void trg_dialog_button_clicked(GtkButton *button, TrgDialog *dialog)
{
    trg_dialog_response(dialog,
                        GPOINTER_TO_INT(g_object_get_data(G_OBJECT(button), RESPONSE_ID_KEY)));
}

GtkWidget *trg_dialog_add_button(TrgDialog *dialog, const gchar *label, gint response_id)
{
    TrgDialogPrivate *priv = trg_dialog_get_instance_private(dialog);
    GtkWidget *button = gtk_button_new_with_mnemonic(label);

    g_object_set_data(G_OBJECT(button), RESPONSE_ID_KEY, GINT_TO_POINTER(response_id));
    g_signal_connect(button, "clicked", G_CALLBACK(trg_dialog_button_clicked), dialog);
    gtk_box_append(GTK_BOX(priv->action_area), button);
    gtk_widget_set_visible(priv->action_area, TRUE);

    return button;
}

GtkWidget *trg_dialog_get_widget_for_response(TrgDialog *dialog, gint response_id)
{
    TrgDialogPrivate *priv = trg_dialog_get_instance_private(dialog);
    GtkWidget *child;

    for (child = gtk_widget_get_first_child(priv->action_area); child;
         child = gtk_widget_get_next_sibling(child)) {
        if (GPOINTER_TO_INT(g_object_get_data(G_OBJECT(child), RESPONSE_ID_KEY)) == response_id)
            return child;
    }

    return NULL;
}

void trg_dialog_set_default_response(TrgDialog *dialog, gint response_id)
{
    GtkWidget *button = trg_dialog_get_widget_for_response(dialog, response_id);

    if (button) {
        gtk_widget_add_css_class(button, "suggested-action");
        gtk_window_set_default_widget(GTK_WINDOW(dialog), button);
    }
}

/* Closing the window is a GTK_RESPONSE_DELETE_EVENT response, like GtkDialog.
 * The handlers usually destroy the window themselves. */
static gboolean trg_dialog_close_request(GtkWindow *window)
{
    TrgDialog *dialog = TRG_DIALOG(window);
    TrgDialogPrivate *priv = trg_dialog_get_instance_private(dialog);
    gboolean destroyed;

    g_object_ref(dialog);
    trg_dialog_response(dialog, GTK_RESPONSE_DELETE_EVENT);
    destroyed = priv->destroyed;
    g_object_unref(dialog);

    if (destroyed)
        return TRUE;

    return GTK_WINDOW_CLASS(trg_dialog_parent_class)->close_request(window);
}

static void trg_dialog_dispose(GObject *object)
{
    TrgDialogPrivate *priv = trg_dialog_get_instance_private(TRG_DIALOG(object));

    priv->destroyed = TRUE;

    G_OBJECT_CLASS(trg_dialog_parent_class)->dispose(object);
}

static void trg_dialog_class_init(TrgDialogClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);
    GtkWidgetClass *widget_class = GTK_WIDGET_CLASS(klass);
    GtkWindowClass *window_class = GTK_WINDOW_CLASS(klass);

    object_class->dispose = trg_dialog_dispose;
    window_class->close_request = trg_dialog_close_request;

    gtk_widget_class_add_binding_action(widget_class, GDK_KEY_Escape, 0, "window.close", NULL);

    signals[SIGNAL_RESPONSE] = g_signal_new("response", G_TYPE_FROM_CLASS(klass), G_SIGNAL_RUN_LAST,
                                            0, NULL, NULL, NULL, G_TYPE_NONE, 1, G_TYPE_INT);
}

static void trg_dialog_init(TrgDialog *self)
{
    TrgDialogPrivate *priv = trg_dialog_get_instance_private(self);
    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);

    priv->content_area = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_widget_set_vexpand(priv->content_area, TRUE);
    gtk_box_append(GTK_BOX(box), priv->content_area);

    priv->action_area = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, GUI_PAD);
    gtk_widget_set_halign(priv->action_area, GTK_ALIGN_END);
    gtk_widget_set_margin_start(priv->action_area, GUI_PAD_BIG);
    gtk_widget_set_margin_end(priv->action_area, GUI_PAD_BIG);
    gtk_widget_set_margin_top(priv->action_area, GUI_PAD);
    gtk_widget_set_margin_bottom(priv->action_area, GUI_PAD_BIG);
    gtk_widget_set_visible(priv->action_area, FALSE);
    gtk_box_append(GTK_BOX(box), priv->action_area);

    gtk_window_set_child(GTK_WINDOW(self), box);
    gtk_widget_add_css_class(GTK_WIDGET(self), "dialog");
}
