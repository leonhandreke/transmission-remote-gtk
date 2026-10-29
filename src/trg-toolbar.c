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

#include "trg-main-window.h"
#include "trg-menu-bar.h"
#include "trg-prefs.h"
#include "trg-toolbar.h"

/* GtkToolbar is gone in GTK 4, so this is a GtkBox of buttons with the
 * "toolbar" style class. The buttons activate the main window's actions, so
 * they are made (in)sensitive along with the menu items. */

enum {
    PROP_0,
    PROP_PREFS,
    PROP_MAIN_WINDOW
};

struct _TrgToolbar {
    GtkBox parent;

    TrgPrefs *prefs;
    TrgMainWindow *main_window;
};

G_DEFINE_TYPE(TrgToolbar, trg_toolbar, GTK_TYPE_BOX)

/* GTK_ICON_SIZE_LARGE_TOOLBAR in GTK 3 */
#define TOOLBAR_ICON_PIXEL_SIZE 24

static void trg_toolbar_set_property(GObject *object, guint prop_id, const GValue *value,
                                     GParamSpec *pspec G_GNUC_UNUSED)
{
    TrgToolbar *self = TRG_TOOLBAR(object);
    switch (prop_id) {
    case PROP_PREFS:
        self->prefs = g_value_get_pointer(value);
        break;
    case PROP_MAIN_WINDOW:
        self->main_window = g_value_get_object(value);
        break;
    }
}

static GtkWidget *trg_toolbar_image_new(const gchar *icon)
{
    GtkWidget *img = gtk_image_new_from_icon_name(icon);
    gtk_image_set_pixel_size(GTK_IMAGE(img), TOOLBAR_ICON_PIXEL_SIZE);
    return img;
}

static GtkWidget *trg_toolbar_item_new(TrgToolbar *toolbar, gchar *text, const gchar *action,
                                       gchar *icon)
{
    GtkWidget *w = gtk_button_new();

    /* GTK_TOOLBAR_BOTH_HORIZ only showed labels for "important" items, so
     * these are icon only, with the label as tooltip. */
    gtk_button_set_child(GTK_BUTTON(w), trg_toolbar_image_new(icon));
    gtk_button_set_has_frame(GTK_BUTTON(w), FALSE);
    gtk_widget_set_tooltip_text(w, text);
    gtk_accessible_update_property(GTK_ACCESSIBLE(w), GTK_ACCESSIBLE_PROPERTY_LABEL, text, -1);
    gtk_actionable_set_action_name(GTK_ACTIONABLE(w), action);
    gtk_box_append(GTK_BOX(toolbar), w);
    return w;
}

static void trg_toolbar_separator_new(TrgToolbar *toolbar)
{
    GtkWidget *separator = gtk_separator_new(GTK_ORIENTATION_VERTICAL);
    gtk_widget_set_margin_start(separator, 4);
    gtk_widget_set_margin_end(separator, 4);
    gtk_box_append(GTK_BOX(toolbar), separator);
}

static void trg_toolbar_constructed(GObject *obj)
{
    TrgToolbar *self = TRG_TOOLBAR(obj);
    GtkWidget *connect, *connectBox, *connectMenuButton;
    GMenu *menu;

    G_OBJECT_CLASS(trg_toolbar_parent_class)->constructed(obj);

    gtk_widget_add_css_class(GTK_WIDGET(obj), "toolbar");

    /* GtkMenuToolButton: a button connecting to the current profile, with a
     * drop down of all the profiles next to it. */
    connectBox = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);
    gtk_widget_add_css_class(connectBox, "linked");

    connect = gtk_button_new();
    {
        GtkWidget *box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
        gtk_box_append(GTK_BOX(box), trg_toolbar_image_new("trg-gtk-connect"));
        gtk_box_append(GTK_BOX(box), gtk_label_new(_("Connect")));
        gtk_button_set_child(GTK_BUTTON(connect), box);
    }
    gtk_button_set_has_frame(GTK_BUTTON(connect), FALSE);
    gtk_widget_set_tooltip_text(connect, _("Connect"));
    gtk_actionable_set_action_target(GTK_ACTIONABLE(connect), "i", -1);
    gtk_actionable_set_action_name(GTK_ACTIONABLE(connect), "win.connect");
    gtk_box_append(GTK_BOX(connectBox), connect);

    connectMenuButton = gtk_menu_button_new();
    gtk_menu_button_set_has_frame(GTK_MENU_BUTTON(connectMenuButton), FALSE);
    menu = trg_menu_bar_file_connect_menu_new(self->main_window, self->prefs);
    gtk_menu_button_set_menu_model(GTK_MENU_BUTTON(connectMenuButton), G_MENU_MODEL(menu));
    g_object_unref(menu);
    gtk_box_append(GTK_BOX(connectBox), connectMenuButton);

    gtk_box_append(GTK_BOX(obj), connectBox);

    trg_toolbar_item_new(self, _("Disconnect"), "win.disconnect", "trg-gtk-disconnect");
    trg_toolbar_item_new(self, _("Add"), "win.add", "list-add");

    trg_toolbar_separator_new(self);

    trg_toolbar_item_new(self, _("Resume"), "win.resume", "media-playback-start");
    trg_toolbar_item_new(self, _("Pause"), "win.pause", "media-playback-pause");
    trg_toolbar_item_new(self, _("Properties"), "win.properties", "document-properties");
    trg_toolbar_item_new(self, _("Remove"), "win.remove", "list-remove");
    trg_toolbar_item_new(self, _("Remove and delete data"), "win.delete", "edit-delete");

    trg_toolbar_separator_new(self);

    trg_toolbar_item_new(self, _("Local Preferences"), "win.local-prefs", "preferences-system");
    trg_toolbar_item_new(self, _("Remote Preferences"), "win.remote-prefs", "network-workgroup");
}

static void trg_toolbar_class_init(TrgToolbarClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);
    object_class->set_property = trg_toolbar_set_property;
    object_class->constructed = trg_toolbar_constructed;

    g_object_class_install_property(
        object_class, PROP_PREFS,
        g_param_spec_pointer("prefs", "Prefs", "Prefs",
                             G_PARAM_WRITABLE | G_PARAM_CONSTRUCT | G_PARAM_STATIC_NAME
                                 | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    g_object_class_install_property(
        object_class, PROP_MAIN_WINDOW,
        g_param_spec_object("mainwindow", "mainwindow", "mainwindow", TRG_TYPE_MAIN_WINDOW,
                            G_PARAM_WRITABLE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));
}

static void trg_toolbar_init(TrgToolbar *self)
{
}

TrgToolbar *trg_toolbar_new(TrgMainWindow *win, TrgPrefs *prefs)
{
    return g_object_new(TRG_TYPE_TOOLBAR, "prefs", prefs, "mainwindow", win, "orientation",
                        GTK_ORIENTATION_HORIZONTAL, NULL);
}
