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
#pragma once

#include <gtk/gtk.h>

G_BEGIN_DECLS

/* A modal-capable window with a content area and a row of buttons, which
 * replaces the deprecated GtkDialog. Like GtkDialog, it emits "response" with
 * the id of the button that was clicked, or GTK_RESPONSE_DELETE_EVENT when it
 * is closed (with the window's close button or Escape). */
#define TRG_TYPE_DIALOG trg_dialog_get_type()
G_DECLARE_DERIVABLE_TYPE(TrgDialog, trg_dialog, TRG, DIALOG, GtkWindow)

struct _TrgDialogClass {
    GtkWindowClass parent_class;
};

GtkWidget *trg_dialog_get_content_area(TrgDialog *dialog);
GtkWidget *trg_dialog_add_button(TrgDialog *dialog, const gchar *label, gint response_id);
void trg_dialog_set_default_response(TrgDialog *dialog, gint response_id);
GtkWidget *trg_dialog_get_widget_for_response(TrgDialog *dialog, gint response_id);
void trg_dialog_response(TrgDialog *dialog, gint response_id);

G_END_DECLS
