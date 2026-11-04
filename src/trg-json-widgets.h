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

#ifndef TRG_JSON_WIDGETS_H_
#define TRG_JSON_WIDGETS_H_

#include <gtk/gtk.h>
#include <json-glib/json-glib.h>

typedef struct {
    GtkWidget *widget;
    gchar *key;
    void (*saveFunc)(GtkWidget *widget, JsonObject *obj, gchar *key);
} trg_json_widget_desc;

void trg_json_widget_add(GList **wl, GtkWidget *w, JsonObject *obj, const gchar *key);
void trg_json_widget_check_save(GtkWidget *widget, JsonObject *obj, gchar *key);
void trg_json_widget_entry_save(GtkWidget *widget, JsonObject *obj, gchar *key);
void trg_json_widget_spin_int_save(GtkWidget *widget, JsonObject *obj, gchar *key);
void trg_json_widget_spin_double_save(GtkWidget *widget, JsonObject *obj, gchar *key);

void trg_json_widget_desc_free(trg_json_widget_desc *wd);
void trg_json_widget_desc_list_free(GList *list);
void trg_json_widgets_save(GList *list, JsonObject *out);

#endif /* TRG_JSON_WIDGETS_H_ */
