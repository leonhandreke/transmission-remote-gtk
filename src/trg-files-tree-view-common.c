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

#include <stdlib.h>

#include <glib.h>
#include <glib/gi18n.h>
#include <gtk/gtk.h>

#include "protocol-constants.h"
#include "trg-files-model-common.h"
#include "trg-files-tree-view-common.h"
#include "util.h"

typedef void (*TrgFilesMenuCallback)(GtkWidget *w, gpointer data);

static void expand_all_cb(GtkWidget *w, gpointer data)
{
    gtk_tree_view_expand_all(GTK_TREE_VIEW(data));
}

static void collapse_all_cb(GtkWidget *w, gpointer data)
{
    gtk_tree_view_collapse_all(GTK_TREE_VIEW(data));
}

static unsigned get_selected_rows_count(GtkTreeView *tv)
{
    GtkTreeSelection *selection = gtk_tree_view_get_selection(tv);
    return gtk_tree_selection_count_selected_rows(selection);
}

static void files_menu_action_cb(GSimpleAction *action, GVariant *parameter, gpointer treeview)
{
    TrgFilesMenuCallback cb
        = (TrgFilesMenuCallback)g_object_get_data(G_OBJECT(action), "trg-files-menu-cb");

    cb(NULL, treeview);
}

static void files_menu_append(GMenu *menu, GActionMap *actions, const gchar *name,
                              const gchar *label, GCallback cb, GtkWidget *treeview,
                              gboolean sensitive)
{
    GSimpleAction *action = g_simple_action_new(name, NULL);
    gchar *detailed = g_strconcat("files.", name, NULL);

    g_object_set_data(G_OBJECT(action), "trg-files-menu-cb", (gpointer)cb);
    g_signal_connect(action, "activate", G_CALLBACK(files_menu_action_cb), treeview);
    g_simple_action_set_enabled(action, sensitive);
    g_action_map_add_action(actions, G_ACTION(action));
    g_object_unref(action);

    g_menu_append(menu, label, detailed);
    g_free(detailed);
}

static void view_popup_menu(GtkWidget *treeview, gdouble x, gdouble y, GCallback rename_cb,
                            GCallback low_cb, GCallback normal_cb, GCallback high_cb,
                            GCallback wanted_cb, GCallback unwanted_cb, gpointer data G_GNUC_UNUSED)
{
    GSimpleActionGroup *actions = g_simple_action_group_new();
    GActionMap *map = G_ACTION_MAP(actions);
    GMenu *menu = g_menu_new();
    GMenu *section;

    if (rename_cb) {
        section = g_menu_new();
        /* Rename on multiple files currently not supported */
        files_menu_append(section, map, "rename", _("Rename..."), rename_cb, treeview,
                          get_selected_rows_count(GTK_TREE_VIEW(treeview)) == 1);
        g_menu_append_section(menu, NULL, G_MENU_MODEL(section));
        g_object_unref(section);
    }

    section = g_menu_new();
    files_menu_append(section, map, "priority-high", _("High Priority"), high_cb, treeview, TRUE);
    files_menu_append(section, map, "priority-normal", _("Normal Priority"), normal_cb, treeview,
                      TRUE);
    files_menu_append(section, map, "priority-low", _("Low Priority"), low_cb, treeview, TRUE);
    g_menu_append_section(menu, NULL, G_MENU_MODEL(section));
    g_object_unref(section);

    section = g_menu_new();
    files_menu_append(section, map, "wanted", _("Download"), wanted_cb, treeview, TRUE);
    files_menu_append(section, map, "unwanted", _("Skip"), unwanted_cb, treeview, TRUE);
    g_menu_append_section(menu, NULL, G_MENU_MODEL(section));
    g_object_unref(section);

    section = g_menu_new();
    files_menu_append(section, map, "expand-all", _("Expand All"), G_CALLBACK(expand_all_cb),
                      treeview, TRUE);
    files_menu_append(section, map, "collapse-all", _("Collapse All"), G_CALLBACK(collapse_all_cb),
                      treeview, TRUE);
    g_menu_append_section(menu, NULL, G_MENU_MODEL(section));
    g_object_unref(section);

    trg_popover_menu_popup(treeview, G_MENU_MODEL(menu), x, y, "files", G_ACTION_GROUP(actions));

    g_object_unref(menu);
    g_object_unref(actions);
}

gboolean trg_files_tree_view_viewOnPopupMenu(GtkWidget *treeview, GCallback rename_cb,
                                             GCallback low_cb, GCallback normal_cb,
                                             GCallback high_cb, GCallback wanted_cb,
                                             GCallback unwanted_cb, gpointer userdata)
{
    view_popup_menu(treeview, -1, -1, rename_cb, low_cb, normal_cb, high_cb, wanted_cb, unwanted_cb,
                    userdata);
    return TRUE;
}

static gboolean onViewPathToggled(GtkTreeView *view, GtkTreeViewColumn *col, GtkTreePath *path,
                                  gint pri_id, gint enabled_id, gpointer data)
{
    int cid;
    gboolean handled = FALSE;

    if (!col || !path)
        return FALSE;

    cid = gtk_tree_view_column_get_sort_column_id(col);
    if ((cid == pri_id) || (cid == enabled_id)) {
        GtkTreeIter iter;
        GtkTreeModel *model = gtk_tree_view_get_model(view);

        gtk_tree_model_get_iter(model, &iter, path);

        if (cid == pri_id) {
            int priority;
            gtk_tree_model_get(model, &iter, pri_id, &priority, -1);
            switch (priority) {
            case TR_PRI_NORMAL:
                priority = TR_PRI_HIGH;
                break;
            case TR_PRI_HIGH:
                priority = TR_PRI_LOW;
                break;
            default:
                priority = TR_PRI_NORMAL;
                break;
            }
            trg_files_tree_model_set_subtree(model, path, &iter, pri_id, priority);
        } else if (cid == enabled_id) {
            int enabled;
            gtk_tree_model_get(model, &iter, enabled_id, &enabled, -1);
            enabled = !enabled;

            trg_files_tree_model_set_subtree(model, path, &iter, enabled_id, enabled);
        }

        handled = TRUE;
    }

    return handled;
}

static gboolean getAndSelectEventPath(GtkTreeView *treeview, gint bx, gint by,
                                      GtkTreeViewColumn **col, GtkTreePath **path)
{
    GtkTreeSelection *sel;

    if (gtk_tree_view_get_path_at_pos(treeview, bx, by, path, col, NULL, NULL)) {
        sel = gtk_tree_view_get_selection(treeview);
        if (!gtk_tree_selection_path_is_selected(sel, *path)) {
            gtk_tree_selection_unselect_all(sel);
            gtk_tree_selection_select_path(sel, *path);
        }
        return TRUE;
    }

    return FALSE;
}

gboolean trg_files_tree_view_onViewButtonPressed(GtkWidget *w, guint button, GdkModifierType state,
                                                 gdouble x, gdouble y, gint pri_id, gint enabled_id,
                                                 GCallback rename_cb, GCallback low_cb,
                                                 GCallback normal_cb, GCallback high_cb,
                                                 GCallback wanted_cb, GCallback unwanted_cb,
                                                 gpointer gdata)
{
    GtkTreeViewColumn *col = NULL;
    GtkTreePath *path = NULL;
    GtkTreeSelection *selection;
    gboolean handled = FALSE;
    GtkTreeView *treeview = GTK_TREE_VIEW(w);
    gint bx, by;

    /* event coordinates are relative to the widget, not the bin window */
    gtk_tree_view_convert_widget_to_bin_window_coords(treeview, (gint)x, (gint)y, &bx, &by);

    if (button == GDK_BUTTON_PRIMARY && !(state & (GDK_SHIFT_MASK | GDK_CONTROL_MASK))
        && getAndSelectEventPath(treeview, bx, by, &col, &path)) {
        handled = onViewPathToggled(treeview, col, path, pri_id, enabled_id, NULL);
    } else if (button == GDK_BUTTON_SECONDARY) {
        selection = gtk_tree_view_get_selection(GTK_TREE_VIEW(treeview));

        if (gtk_tree_view_get_path_at_pos(GTK_TREE_VIEW(treeview), bx, by, &path, NULL, NULL,
                                          NULL)) {
            if (!gtk_tree_selection_path_is_selected(selection, path)) {
                gtk_tree_selection_unselect_all(selection);
                gtk_tree_selection_select_path(selection, path);
            }

            view_popup_menu(w, x, y, rename_cb, low_cb, normal_cb, high_cb, wanted_cb, unwanted_cb,
                            gdata);
            handled = TRUE;
        }
    }

    gtk_tree_path_free(path);

    return handled;
}
