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

#include "trg-about-window.h"

GtkWidget *trg_about_window_new(GtkWindow *parent)
{
    GtkBuilder *builder
        = gtk_builder_new_from_resource("/io/github/TransmissionRemoteGtk/ui/trg-about-window.ui");
    GtkWidget *dialog = GTK_WIDGET(gtk_builder_get_object(builder, "about_dialog"));

    /* Toplevel windows stay around until they're destroyed */
    g_object_unref(builder);

    gtk_window_set_transient_for(GTK_WINDOW(dialog), parent);
    gtk_about_dialog_set_logo_icon_name(GTK_ABOUT_DIALOG(dialog), PACKAGE_NAME);
    gtk_about_dialog_set_program_name(GTK_ABOUT_DIALOG(dialog), PACKAGE_NAME);
    gtk_about_dialog_set_version(GTK_ABOUT_DIALOG(dialog), PACKAGE_VERSION);

    return dialog;
}
