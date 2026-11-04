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
    GtkWidget *connect_menu_button;
};

G_DEFINE_TYPE(TrgToolbar, trg_toolbar, GTK_TYPE_BOX)

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

static void trg_toolbar_constructed(GObject *obj)
{
    TrgToolbar *self = TRG_TOOLBAR(obj);
    GMenu *menu;

    G_OBJECT_CLASS(trg_toolbar_parent_class)->constructed(obj);

    menu = trg_menu_bar_file_connect_menu_new(self->main_window, self->prefs);
    gtk_menu_button_set_menu_model(GTK_MENU_BUTTON(self->connect_menu_button), G_MENU_MODEL(menu));
    g_object_unref(menu);
}

static void trg_toolbar_dispose(GObject *object)
{
    gtk_widget_dispose_template(GTK_WIDGET(object), TRG_TYPE_TOOLBAR);

    G_OBJECT_CLASS(trg_toolbar_parent_class)->dispose(object);
}

static void trg_toolbar_class_init(TrgToolbarClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);
    GtkWidgetClass *widget_class = GTK_WIDGET_CLASS(klass);

    object_class->set_property = trg_toolbar_set_property;
    object_class->constructed = trg_toolbar_constructed;
    object_class->dispose = trg_toolbar_dispose;

    gtk_widget_class_set_template_from_resource(
        widget_class, "/io/github/TransmissionRemoteGtk/ui/trg-toolbar.ui");
    gtk_widget_class_bind_template_child(widget_class, TrgToolbar, connect_menu_button);

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
    gtk_widget_init_template(GTK_WIDGET(self));
}

TrgToolbar *trg_toolbar_new(TrgMainWindow *win, TrgPrefs *prefs)
{
    return g_object_new(TRG_TYPE_TOOLBAR, "prefs", prefs, "mainwindow", win, NULL);
}
