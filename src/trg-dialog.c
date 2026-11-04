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
#include <string.h>

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

static void trg_dialog_buildable_init(GtkBuildableIface *iface);

G_DEFINE_TYPE_WITH_CODE(TrgDialog, trg_dialog, GTK_TYPE_WINDOW,
                        G_ADD_PRIVATE(TrgDialog)
                            G_IMPLEMENT_INTERFACE(GTK_TYPE_BUILDABLE, trg_dialog_buildable_init))

static GtkBuildableIface *parent_buildable_iface;

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

static void trg_dialog_append_action(TrgDialog *dialog, GtkWidget *widget)
{
    TrgDialogPrivate *priv = trg_dialog_get_instance_private(dialog);

    gtk_box_append(GTK_BOX(priv->action_area), widget);
    gtk_widget_set_visible(priv->action_area, TRUE);
}

GtkWidget *trg_dialog_add_button(TrgDialog *dialog, const gchar *label, gint response_id)
{
    GtkWidget *button = gtk_button_new_with_mnemonic(label);

    g_object_set_data(G_OBJECT(button), RESPONSE_ID_KEY, GINT_TO_POINTER(response_id));
    g_signal_connect(button, "clicked", G_CALLBACK(trg_dialog_button_clicked), dialog);
    trg_dialog_append_action(dialog, button);

    return button;
}

/* A response id from its GtkResponseType nick, like "accept" */
static gint trg_dialog_response_from_nick(const gchar *nick)
{
    GEnumClass *enum_class = g_type_class_ref(GTK_TYPE_RESPONSE_TYPE);
    GEnumValue *value = g_enum_get_value_by_nick(enum_class, nick);
    gint response_id = value ? value->value : GTK_RESPONSE_NONE;

    if (!value)
        g_warning("Unknown dialog response \"%s\"", nick);

    g_type_class_unref(enum_class);

    return response_id;
}

/* The responses of the buttons of a template: "dialog.accept" responds with
 * GTK_RESPONSE_ACCEPT, and so on. They're separate actions, so a response can
 * be disabled (and its button made insensitive) with
 * gtk_widget_action_set_enabled(). */
static const gchar *response_actions[]
    = { "accept", "cancel", "ok", "close", "apply", "yes", "no" };

static gint trg_dialog_response_from_action(const gchar *action_name)
{
    if (!action_name || !g_str_has_prefix(action_name, "dialog."))
        return GTK_RESPONSE_NONE;

    return trg_dialog_response_from_nick(action_name + strlen("dialog."));
}

static void trg_dialog_response_action(GtkWidget *widget, const gchar *action_name,
                                       GVariant *parameter)
{
    trg_dialog_response(TRG_DIALOG(widget), trg_dialog_response_from_action(action_name));
}

/* In a template, children go into the content area, and children of type
 * "action" ([action] in Blueprint) into the row of buttons. Buttons there
 * respond through the dialog.* actions, which is also how
 * trg_dialog_get_widget_for_response() finds them. */
static void trg_dialog_buildable_add_child(GtkBuildable *buildable, GtkBuilder *builder,
                                           GObject *child, const gchar *type)
{
    TrgDialog *dialog = TRG_DIALOG(buildable);
    TrgDialogPrivate *priv = trg_dialog_get_instance_private(dialog);

    if (g_strcmp0(type, "action") == 0) {
        if (GTK_IS_ACTIONABLE(child)) {
            const gchar *name = gtk_actionable_get_action_name(GTK_ACTIONABLE(child));
            gint response_id = trg_dialog_response_from_action(name);

            if (response_id != GTK_RESPONSE_NONE)
                g_object_set_data(child, RESPONSE_ID_KEY, GINT_TO_POINTER(response_id));
        }
        trg_dialog_append_action(dialog, GTK_WIDGET(child));
    } else if (!type && GTK_IS_WIDGET(child) && priv->content_area) {
        gtk_box_append(GTK_BOX(priv->content_area), GTK_WIDGET(child));
    } else {
        parent_buildable_iface->add_child(buildable, builder, child, type);
    }
}

static void trg_dialog_buildable_init(GtkBuildableIface *iface)
{
    parent_buildable_iface = g_type_interface_peek_parent(iface);
    iface->add_child = trg_dialog_buildable_add_child;
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
    guint i;

    object_class->dispose = trg_dialog_dispose;
    window_class->close_request = trg_dialog_close_request;

    gtk_widget_class_add_binding_action(widget_class, GDK_KEY_Escape, 0, "window.close", NULL);
    for (i = 0; i < G_N_ELEMENTS(response_actions); i++) {
        g_autofree gchar *name = g_strconcat("dialog.", response_actions[i], NULL);
        gtk_widget_class_install_action(widget_class, name, NULL, trg_dialog_response_action);
    }

    signals[SIGNAL_RESPONSE] = g_signal_new("response", G_TYPE_FROM_CLASS(klass), G_SIGNAL_RUN_LAST,
                                            0, NULL, NULL, NULL, G_TYPE_NONE, 1, G_TYPE_INT);
}

static void trg_dialog_init(TrgDialog *self)
{
    TrgDialogPrivate *priv = trg_dialog_get_instance_private(self);
    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);

    priv->content_area = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_widget_set_vexpand(priv->content_area, TRUE);
    gtk_widget_set_margin_start(priv->content_area, GUI_PAD);
    gtk_widget_set_margin_end(priv->content_area, GUI_PAD);
    gtk_widget_set_margin_top(priv->content_area, GUI_PAD);
    gtk_widget_set_margin_bottom(priv->content_area, GUI_PAD);
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
