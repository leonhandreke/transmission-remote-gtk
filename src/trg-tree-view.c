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
#include <stdlib.h>

#include "trg-cell-renderer-epoch.h"
#include "trg-cell-renderer-eta.h"
#include "trg-cell-renderer-file-icon.h"
#include "trg-cell-renderer-numgteqthan.h"
#include "trg-cell-renderer-priority.h"
#include "trg-cell-renderer-ratio.h"
#include "trg-cell-renderer-size.h"
#include "trg-cell-renderer-speed.h"
#include "trg-cell-renderer-wanted.h"
#include "trg-prefs.h"
#include "trg-tree-view.h"
#include "util.h"

/* A subclass of GtkTreeView which allows the user to change column visibility
 * by right clicking on any column for a menu to hide the clicked column, or
 * insert any hidden column after.
 *
 * This class persists these choices to TrgPrefs, and restores them when it is
 * initialised. Column widths are also saved/restored.
 *
 * All the columns must be preregistered so it knows what model column,
 * renderers etc to use if it should be created, and what columns are available.
 */

enum {
    PROP_0,
    PROP_PREFS,
    PROP_CONFIGID
};

enum {
    SIGNAL_COLUMN_ADDED,
    SIGNAL_COUNT
};

static guint signals[SIGNAL_COUNT] = { 0 };

typedef struct {
    GList *columns;
    TrgPrefs *prefs;
    gchar *configId;
} TrgTreeViewPrivate;

G_DEFINE_TYPE_WITH_PRIVATE(TrgTreeView, trg_tree_view, GTK_TYPE_TREE_VIEW)

#define GDATA_KEY_COLUMN_DESC "column-desc"

gboolean trg_tree_view_is_column_showing(TrgTreeView *tv, gint index)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);

    GList *li;
    for (li = priv->columns; li; li = g_list_next(li)) {
        trg_column_description *cd = (trg_column_description *)li->data;
        if (cd->model_column == index) {
            if (cd->flags & TRG_COLUMN_SHOWING)
                return TRUE;
            else
                break;
        }
    }

    return FALSE;
}

static void trg_column_description_free(trg_column_description *desc)
{
    if (desc) {
        g_free(desc->header);
        g_free(desc->id);
        g_free(desc);
    }
}

static void trg_tree_view_get_property(GObject *object, guint property_id, GValue *value,
                                       GParamSpec *pspec)
{
    G_OBJECT_WARN_INVALID_PROPERTY_ID(object, property_id, pspec);
}

static void trg_tree_view_set_property(GObject *object, guint property_id, const GValue *value,
                                       GParamSpec *pspec)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(TRG_TREE_VIEW(object));
    switch (property_id) {
    case PROP_PREFS:
        priv->prefs = g_value_get_object(value);
        break;
    case PROP_CONFIGID:
        g_free(priv->configId);
        priv->configId = g_value_dup_string(value);
        break;
    default:
        G_OBJECT_WARN_INVALID_PROPERTY_ID(object, property_id, pspec);
        break;
    }
}

static GObject *trg_tree_view_constructor(GType type, guint n_construct_properties,
                                          GObjectConstructParam *construct_params)
{
    GObject *obj = G_OBJECT_CLASS(trg_tree_view_parent_class)
                       ->constructor(type, n_construct_properties, construct_params);

    return obj;
}

static JsonObject *trg_prefs_get_tree_view_props(TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    JsonObject *root = trg_prefs_get_root(priv->prefs);
    const gchar *className
        = priv->configId && strlen(priv->configId) > 0 ? priv->configId : G_OBJECT_TYPE_NAME(tv);
    JsonObject *obj;
    JsonObject *tvProps = NULL;

    if (!json_object_has_member(root, TRG_PREFS_KEY_TREE_VIEWS)) {
        obj = json_object_new();
        json_object_set_object_member(root, TRG_PREFS_KEY_TREE_VIEWS, obj);
    } else {
        obj = json_object_get_object_member(root, TRG_PREFS_KEY_TREE_VIEWS);
    }

    if (!json_object_has_member(obj, className)) {
        tvProps = json_object_new();
        json_object_set_object_member(obj, className, tvProps);
    } else {
        tvProps = json_object_get_object_member(obj, className);
    }

    return tvProps;
}

static void trg_tree_view_add_column_after(TrgTreeView *tv, trg_column_description *desc,
                                           gint64 width, GtkTreeViewColumn *after_col);

trg_column_description *trg_tree_view_reg_column(TrgTreeView *tv, gint type, gint model_column,
                                                 const gchar *header, const gchar *id, guint flags)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    trg_column_description *desc = g_new0(trg_column_description, 1);

    desc->type = type;
    desc->model_column = model_column;
    desc->header = g_strdup(header);
    desc->id = g_strdup(id);
    desc->flags = flags;

    priv->columns = g_list_append(priv->columns, desc);

    return desc;
}

static trg_column_description *trg_tree_view_find_column(TrgTreeView *tv, const gchar *id)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GList *li;
    trg_column_description *desc;

    for (li = priv->columns; li; li = g_list_next(li)) {
        desc = (trg_column_description *)li->data;
        if (!g_strcmp0(desc->id, id))
            return desc;
    }

    return NULL;
}

static void trg_tree_view_hide_column(GtkWidget *w, GtkTreeViewColumn *col)
{
    trg_column_description *desc = g_object_get_data(G_OBJECT(col), GDATA_KEY_COLUMN_DESC);
    GtkWidget *tv = gtk_tree_view_column_get_tree_view(col);
    desc->flags &= ~TRG_COLUMN_SHOWING;
    gtk_tree_view_remove_column(GTK_TREE_VIEW(tv), col);
}

static void trg_tree_view_add_column(TrgTreeView *tv, trg_column_description *desc, gint64 width)
{
    trg_tree_view_add_column_after(tv, desc, width, NULL);
}

static void trg_tree_view_user_add_column_cb(GtkTreeViewColumn *col, trg_column_description *desc)
{
    TrgTreeView *tv = TRG_TREE_VIEW(gtk_tree_view_column_get_tree_view(col));

    trg_tree_view_add_column_after(tv, desc, -1, col);

    g_signal_emit(tv, signals[SIGNAL_COLUMN_ADDED], 0, desc->id);
}

static void trg_tree_view_sort_type_change_state(GSimpleAction *action, GVariant *value,
                                                 gpointer data)
{
    GtkTreeSortable *model = GTK_TREE_SORTABLE(data);
    gint sortColumn;

    gtk_tree_sortable_get_sort_column_id(model, &sortColumn, NULL);
    gtk_tree_sortable_set_sort_column_id(model, sortColumn, g_variant_get_int32(value));
}

static void trg_tree_view_sort_column_change_state(GSimpleAction *action, GVariant *value,
                                                   gpointer data)
{
    GtkTreeSortable *model = GTK_TREE_SORTABLE(data);
    GtkSortType sortType;

    gtk_tree_sortable_get_sort_column_id(model, NULL, &sortType);
    gtk_tree_sortable_set_sort_column_id(model, g_variant_get_int32(value), sortType);
}

/* Keep the radio items in sync when sorting changes elsewhere, eg. by
 * clicking a column header. */
static void trg_tree_view_sort_changed_cb(GtkTreeSortable *model, gpointer data)
{
    GActionMap *map = G_ACTION_MAP(data);
    gint sort;
    GtkSortType sortType;

    gtk_tree_sortable_get_sort_column_id(model, &sort, &sortType);
    g_simple_action_set_state(
        G_SIMPLE_ACTION(g_action_map_lookup_action(map, TRG_TREE_VIEW_SORT_TYPE_ACTION)),
        g_variant_new_int32(sortType));
    g_simple_action_set_state(
        G_SIMPLE_ACTION(g_action_map_lookup_action(map, TRG_TREE_VIEW_SORT_COLUMN_ACTION)),
        g_variant_new_int32(sort));
}

GMenu *trg_tree_view_sort_menu(TrgTreeView *tv, GActionMap *map)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GtkTreeModel *treeViewModel = gtk_tree_view_get_model(GTK_TREE_VIEW(tv));
    GtkTreeSortable *sortableModel
        = GTK_TREE_SORTABLE(gtk_tree_model_filter_get_model(GTK_TREE_MODEL_FILTER(treeViewModel)));
    GMenu *menu = g_menu_new();
    GMenu *section;
    GMenuItem *item;
    GSimpleAction *action;
    GList *li;
    gint sort;
    GtkSortType sortType;

    gtk_tree_sortable_get_sort_column_id(sortableModel, &sort, &sortType);

    action = g_simple_action_new_stateful(TRG_TREE_VIEW_SORT_TYPE_ACTION, G_VARIANT_TYPE_INT32,
                                          g_variant_new_int32(sortType));
    g_signal_connect(action, "change-state", G_CALLBACK(trg_tree_view_sort_type_change_state),
                     sortableModel);
    g_action_map_add_action(map, G_ACTION(action));
    g_object_unref(action);

    action = g_simple_action_new_stateful(TRG_TREE_VIEW_SORT_COLUMN_ACTION, G_VARIANT_TYPE_INT32,
                                          g_variant_new_int32(sort));
    g_signal_connect(action, "change-state", G_CALLBACK(trg_tree_view_sort_column_change_state),
                     sortableModel);
    g_action_map_add_action(map, G_ACTION(action));
    g_object_unref(action);

    g_signal_connect_object(sortableModel, "sort-column-changed",
                            G_CALLBACK(trg_tree_view_sort_changed_cb), map, 0);

    section = g_menu_new();
    item = g_menu_item_new(_("Ascending"), NULL);
    g_menu_item_set_action_and_target(item, "win." TRG_TREE_VIEW_SORT_TYPE_ACTION, "i",
                                      GTK_SORT_ASCENDING);
    g_menu_append_item(section, item);
    g_object_unref(item);
    item = g_menu_item_new(_("Descending"), NULL);
    g_menu_item_set_action_and_target(item, "win." TRG_TREE_VIEW_SORT_TYPE_ACTION, "i",
                                      GTK_SORT_DESCENDING);
    g_menu_append_item(section, item);
    g_object_unref(item);
    g_menu_append_section(menu, NULL, G_MENU_MODEL(section));
    g_object_unref(section);

    section = g_menu_new();
    for (li = priv->columns; li; li = g_list_next(li)) {
        trg_column_description *desc = (trg_column_description *)li->data;
        if (!(desc->flags & TRG_COLUMN_HIDE_FROM_TOP_MENU)) {
            item = g_menu_item_new(desc->header, NULL);
            g_menu_item_set_action_and_target(item, "win." TRG_TREE_VIEW_SORT_COLUMN_ACTION, "i",
                                              desc->model_column);
            g_menu_append_item(section, item);
            g_object_unref(item);
        }
    }
    g_menu_append_section(menu, NULL, G_MENU_MODEL(section));
    g_object_unref(section);

    return menu;
}

static void column_menu_hide_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    trg_tree_view_hide_column(NULL, GTK_TREE_VIEW_COLUMN(data));
}

static void column_menu_add_cb(GSimpleAction *action, GVariant *parameter, gpointer data)
{
    trg_tree_view_user_add_column_cb(GTK_TREE_VIEW_COLUMN(data),
                                     g_object_get_data(G_OBJECT(action), GDATA_KEY_COLUMN_DESC));
}

static void view_popup_menu(GtkWidget *button, gdouble x, gdouble y, GtkTreeViewColumn *column)
{
    GtkWidget *tv = gtk_tree_view_column_get_tree_view(column);
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(TRG_TREE_VIEW(tv));
    GSimpleActionGroup *actions = g_simple_action_group_new();
    GMenu *menu = g_menu_new();
    GSimpleAction *action;
    trg_column_description *desc;
    graphene_point_t point;
    guint n_showing = 0;
    GList *li;
    gint i;

    desc = g_object_get_data(G_OBJECT(column), GDATA_KEY_COLUMN_DESC);

    /* The shown column comes first, checked, and unchecks (hides) it. The
     * hidden columns follow, unchecked, and add themselves after it. */
    for (li = priv->columns, i = 0; li; li = g_list_next(li), i++) {
        trg_column_description *desc = (trg_column_description *)li->data;
        if (!(desc->flags & TRG_COLUMN_SHOWING)) {
            gchar *name = g_strdup_printf("show-%d", i);
            gchar *detailed = g_strconcat("columns.", name, NULL);

            action = g_simple_action_new_stateful(name, NULL, g_variant_new_boolean(FALSE));
            g_object_set_data(G_OBJECT(action), GDATA_KEY_COLUMN_DESC, desc);
            g_signal_connect(action, "activate", G_CALLBACK(column_menu_add_cb), column);
            g_action_map_add_action(G_ACTION_MAP(actions), G_ACTION(action));
            g_object_unref(action);

            g_menu_append(menu, desc->header, detailed);

            g_free(detailed);
            g_free(name);
        } else {
            n_showing++;
        }
    }

    action = g_simple_action_new_stateful("hide", NULL, g_variant_new_boolean(TRUE));
    g_signal_connect(action, "activate", G_CALLBACK(column_menu_hide_cb), column);
    g_simple_action_set_enabled(action, !(desc->flags & TRG_COLUMN_UNREMOVABLE) && n_showing > 1);
    g_action_map_add_action(G_ACTION_MAP(actions), G_ACTION(action));
    g_object_unref(action);
    g_menu_prepend(menu, desc->header, "columns.hide");

    /* the popover is attached to the tree view, the click was on the header button */
    if (!gtk_widget_compute_point(button, tv, &GRAPHENE_POINT_INIT((float)x, (float)y), &point))
        point.x = point.y = -1;

    trg_popover_menu_popup(tv, G_MENU_MODEL(menu), point.x, point.y, "columns",
                           G_ACTION_GROUP(actions));

    g_object_unref(menu);
    g_object_unref(actions);
}

/* This used to get the column as an argument binded when the signal was
 * created, but it seems GTK now destroys and recreates them in some
 * circumstances. So bind the column description instead and search for the
 * column by its title.
 */
static void col_onButtonPressed(GtkGestureClick *gesture, gint n_press, gdouble x, gdouble y,
                                trg_column_description *desc)
{
    GtkWidget *button = gtk_event_controller_get_widget(GTK_EVENT_CONTROLLER(gesture));
    GtkTreeView *gtv = GTK_TREE_VIEW(gtk_widget_get_parent(button));
    GList *cols = gtk_tree_view_get_columns(gtv);
    GList *li;

    for (li = cols; li; li = g_list_next(li)) {
        GtkTreeViewColumn *col = GTK_TREE_VIEW_COLUMN(li->data);
        if (!g_strcmp0(desc->header, gtk_tree_view_column_get_title(col))) {
            view_popup_menu(button, x, y, col);
            break;
        }
    }
    g_list_free(cols);

    gtk_gesture_set_state(GTK_GESTURE(gesture), GTK_EVENT_SEQUENCE_CLAIMED);
}

static GtkTreeViewColumn *trg_tree_view_icontext_column_new(trg_column_description *desc,
                                                            gchar *renderer_property)
{
    GtkTreeViewColumn *column = gtk_tree_view_column_new();
    GtkCellRenderer *renderer = gtk_cell_renderer_pixbuf_new();

    gtk_tree_view_column_set_title(column, desc->header);
    gtk_tree_view_column_pack_start(column, renderer, FALSE);
    gtk_tree_view_column_set_attributes(column, renderer, renderer_property,
                                        desc->model_column_extra, NULL);

    renderer = gtk_cell_renderer_text_new();
    gtk_tree_view_column_pack_start(column, renderer, TRUE);
    gtk_tree_view_column_set_attributes(column, renderer, "text", desc->model_column, NULL);

    return column;
}

static GtkTreeViewColumn *trg_tree_view_fileicontext_column_new(trg_column_description *desc)
{
    GtkTreeViewColumn *column = gtk_tree_view_column_new();
    GtkCellRenderer *renderer = trg_cell_renderer_file_icon_new();

    gtk_tree_view_column_set_title(column, desc->header);
    gtk_tree_view_column_pack_start(column, renderer, FALSE);
    gtk_tree_view_column_set_attributes(column, renderer, "file-id", desc->model_column_extra,
                                        "file-name", desc->model_column, NULL);

    renderer = gtk_cell_renderer_text_new();
    gtk_tree_view_column_pack_start(column, renderer, TRUE);
    gtk_tree_view_column_set_attributes(column, renderer, "text", desc->model_column, NULL);

    return column;
}

static void trg_tree_view_add_column_after(TrgTreeView *tv, trg_column_description *desc,
                                           gint64 width, GtkTreeViewColumn *after_col)
{
    GtkCellRenderer *renderer;
    GtkTreeViewColumn *column = NULL;
    GtkWidget *column_button = NULL;
    GtkGesture *gesture;

    switch (desc->type) {
    case TRG_COLTYPE_TEXT:
        renderer = desc->customRenderer ? desc->customRenderer : gtk_cell_renderer_text_new();
        column = gtk_tree_view_column_new_with_attributes(desc->header, renderer, "text",
                                                          desc->model_column, NULL);

        break;
    case TRG_COLTYPE_SPEED:
        renderer = trg_cell_renderer_speed_new();
        column = gtk_tree_view_column_new_with_attributes(desc->header, renderer, "speed-value",
                                                          desc->model_column, NULL);
        break;
    case TRG_COLTYPE_EPOCH:
        renderer = trg_cell_renderer_epoch_new();
        column = gtk_tree_view_column_new_with_attributes(desc->header, renderer, "epoch-value",
                                                          desc->model_column, NULL);
        break;
    case TRG_COLTYPE_ETA:
        renderer = trg_cell_renderer_eta_new();
        column = gtk_tree_view_column_new_with_attributes(desc->header, renderer, "eta-value",
                                                          desc->model_column, NULL);
        break;
    case TRG_COLTYPE_SIZE:
        renderer = trg_cell_renderer_size_new();
        column = gtk_tree_view_column_new_with_attributes(desc->header, renderer, "size-value",
                                                          desc->model_column, NULL);
        break;
    case TRG_COLTYPE_PROG:
        renderer = gtk_cell_renderer_progress_new();
        column = gtk_tree_view_column_new_with_attributes(desc->header, renderer, "value",
                                                          desc->model_column, NULL);
        break;
    case TRG_COLTYPE_RATIO:
        renderer = trg_cell_renderer_ratio_new();
        column = gtk_tree_view_column_new_with_attributes(desc->header, renderer, "ratio-value",
                                                          desc->model_column, NULL);
        break;
    case TRG_COLTYPE_WANTED:
        column = gtk_tree_view_column_new();
        renderer = trg_cell_renderer_wanted_new();
        /*gtk_cell_renderer_set_alignment(GTK_CELL_RENDERER(renderer), 0.5f,
           0.0); */
        gtk_tree_view_column_set_title(column, desc->header);
        gtk_tree_view_column_pack_start(column, renderer, TRUE);
        gtk_tree_view_column_set_attributes(column, renderer, "wanted-value", desc->model_column,
                                            NULL);
        break;
    case TRG_COLTYPE_ICONTEXT:
        column = trg_tree_view_icontext_column_new(desc, "icon-name");
        break;
    case TRG_COLTYPE_FILEICONTEXT:
        column = trg_tree_view_fileicontext_column_new(desc);
        break;
    case TRG_COLTYPE_PRIO:
        renderer = trg_cell_renderer_priority_new();
        column = gtk_tree_view_column_new_with_attributes(desc->header, renderer, "priority-value",
                                                          desc->model_column, NULL);
        break;
    case TRG_COLTYPE_NUMGTZERO:
        renderer = trg_cell_renderer_numgteqthan_new(1);
        column = gtk_tree_view_column_new_with_attributes(desc->header, renderer, "value",
                                                          desc->model_column, NULL);
        break;
    case TRG_COLTYPE_NUMGTEQZERO:
        renderer = trg_cell_renderer_numgteqthan_new(0);
        column = gtk_tree_view_column_new_with_attributes(desc->header, renderer, "value",
                                                          desc->model_column, NULL);
        break;
    default:
        g_critical("unknown TrgTreeView column");
        return;
    }

    // gtk_tree_view_column_set_min_width(column, 0);
    gtk_tree_view_column_set_resizable(column, TRUE);
    gtk_tree_view_column_set_reorderable(column, TRUE);
    gtk_tree_view_column_set_sort_column_id(column, desc->model_column);
    gtk_tree_view_column_set_expand(column, TRUE);

    /*gtk_tree_view_column_set_sizing(column,
                                    GTK_TREE_VIEW_COLUMN_FIXED);*/

    if (width > 0) {
        // gtk_tree_view_column_set_fixed_width(column, width);
    }

    g_object_set_data(G_OBJECT(column), GDATA_KEY_COLUMN_DESC, desc);

    gtk_tree_view_append_column(GTK_TREE_VIEW(tv), column);

    if (after_col)
        gtk_tree_view_move_column_after(GTK_TREE_VIEW(tv), column, after_col);

    column_button = gtk_tree_view_column_get_button(column);

    gesture = gtk_gesture_click_new();
    gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture), GDK_BUTTON_SECONDARY);
    g_signal_connect(gesture, "pressed", G_CALLBACK(col_onButtonPressed), desc);
    gtk_widget_add_controller(column_button, GTK_EVENT_CONTROLLER(gesture));

    if (desc->out)
        *(desc->out) = column;

    desc->flags |= TRG_COLUMN_SHOWING;
}

void trg_tree_view_remove_all_columns(TrgTreeView *tv)
{
    GtkTreeView *gtv = GTK_TREE_VIEW(tv);
    GList *cols = gtk_tree_view_get_columns(gtv);
    GList *li;
    for (li = cols; li; li = g_list_next(li)) {
        gtk_tree_view_remove_column(gtv, GTK_TREE_VIEW_COLUMN(li->data));
    }
    g_list_free(cols);
}

void trg_tree_view_persist(TrgTreeView *tv, guint flags)
{
    JsonObject *props = trg_prefs_get_tree_view_props(tv);
    GtkTreeModel *model = gtk_tree_view_get_model(GTK_TREE_VIEW(tv));
    GList *cols, *li;
    JsonArray *widths, *columns;
    gint sort_column_id;
    GtkSortType sort_type;

    if (flags & TRG_TREE_VIEW_PERSIST_SORT) {
        gtk_tree_sortable_get_sort_column_id(
            GTK_TREE_SORTABLE((flags & TRG_TREE_VIEW_SORTABLE_PARENT)
                                  ? gtk_tree_model_filter_get_model(GTK_TREE_MODEL_FILTER(model))
                                  : model),
            &sort_column_id, &sort_type);

        if (json_object_has_member(props, TRG_PREFS_KEY_TV_SORT_COL))
            json_object_remove_member(props, TRG_PREFS_KEY_TV_SORT_COL);

        if (json_object_has_member(props, TRG_PREFS_KEY_TV_SORT_TYPE))
            json_object_remove_member(props, TRG_PREFS_KEY_TV_SORT_TYPE);

        json_object_set_int_member(props, TRG_PREFS_KEY_TV_SORT_COL, (gint64)sort_column_id);
        json_object_set_int_member(props, TRG_PREFS_KEY_TV_SORT_TYPE, (gint64)sort_type);
    }

    if (flags & TRG_TREE_VIEW_PERSIST_LAYOUT) {
        cols = gtk_tree_view_get_columns(GTK_TREE_VIEW(tv));

        if (json_object_has_member(props, TRG_PREFS_KEY_TV_WIDTHS))
            json_object_remove_member(props, TRG_PREFS_KEY_TV_WIDTHS);

        widths = json_array_new();
        json_object_set_array_member(props, TRG_PREFS_KEY_TV_WIDTHS, widths);

        if (json_object_has_member(props, TRG_PREFS_KEY_TV_COLUMNS))
            json_object_remove_member(props, TRG_PREFS_KEY_TV_COLUMNS);

        columns = json_array_new();
        json_object_set_array_member(props, TRG_PREFS_KEY_TV_COLUMNS, columns);

        for (li = cols; li; li = g_list_next(li)) {
            GtkTreeViewColumn *col = (GtkTreeViewColumn *)li->data;
            trg_column_description *desc
                = g_object_get_data(G_OBJECT(li->data), GDATA_KEY_COLUMN_DESC);

            json_array_add_string_element(columns, desc->id);
            json_array_add_int_element(widths, gtk_tree_view_column_get_width(col));
        }

        g_list_free(cols);
    }
}

void trg_tree_view_restore_sort(TrgTreeView *tv, guint flags)
{
    JsonObject *props = trg_prefs_get_tree_view_props(tv);
    GtkTreeModel *model = gtk_tree_view_get_model(GTK_TREE_VIEW(tv));

    if (json_object_has_member(props, TRG_PREFS_KEY_TV_SORT_COL)
        && json_object_has_member(props, TRG_PREFS_KEY_TV_SORT_TYPE)) {
        gint64 sort_col = json_object_get_int_member(props, TRG_PREFS_KEY_TV_SORT_COL);
        gint64 sort_type = json_object_get_int_member(props, TRG_PREFS_KEY_TV_SORT_TYPE);
        gtk_tree_sortable_set_sort_column_id(
            GTK_TREE_SORTABLE((flags & TRG_TREE_VIEW_SORTABLE_PARENT)
                                  ? gtk_tree_model_filter_get_model(GTK_TREE_MODEL_FILTER(model))
                                  : model),
            sort_col, (GtkSortType)sort_type);
    }
}

void trg_tree_view_set_prefs(TrgTreeView *tv, TrgPrefs *prefs)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    priv->prefs = prefs;
}

void trg_tree_view_setup_columns(TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    JsonObject *props = trg_prefs_get_tree_view_props(tv);
    GList *columns, *widths, *cli, *wli;

    if (!json_object_has_member(props, TRG_PREFS_KEY_TV_COLUMNS)
        || !json_object_has_member(props, TRG_PREFS_KEY_TV_WIDTHS)) {
        GList *li;
        for (li = priv->columns; li; li = g_list_next(li)) {
            trg_column_description *desc = (trg_column_description *)li->data;
            if (desc && !(desc->flags & TRG_COLUMN_EXTRA))
                trg_tree_view_add_column(tv, desc, -1);
        }
        return;
    }

    columns
        = json_array_get_elements(json_object_get_array_member(props, TRG_PREFS_KEY_TV_COLUMNS));
    widths = json_array_get_elements(json_object_get_array_member(props, TRG_PREFS_KEY_TV_WIDTHS));

    for (cli = columns, wli = widths; cli && wli; cli = g_list_next(cli), wli = g_list_next(wli)) {
        trg_column_description *desc
            = trg_tree_view_find_column(tv, json_node_get_string((JsonNode *)cli->data));
        if (desc) {
            gint64 width = json_node_get_int((JsonNode *)wli->data);
            trg_tree_view_add_column(tv, desc, width);
        }
    }

    g_list_free(columns);
    g_list_free(widths);
}

GList *trg_tree_view_get_selected_refs_list(GtkTreeView *tv)
{
    GtkTreeModel *model = gtk_tree_view_get_model(tv);
    GtkTreeSelection *selection = gtk_tree_view_get_selection(tv);
    GList *li, *selectionList;
    GList *refList = NULL;

    selectionList = gtk_tree_selection_get_selected_rows(selection, NULL);
    for (li = selectionList; li != NULL; li = g_list_next(li)) {
        GtkTreePath *path = (GtkTreePath *)li->data;
        GtkTreeRowReference *ref = gtk_tree_row_reference_new(model, path);
        gtk_tree_path_free(path);
        refList = g_list_append(refList, ref);
    }
    g_list_free(selectionList);

    return refList;
}

static void trg_tree_view_dispose(GObject *object)
{
    G_OBJECT_CLASS(trg_tree_view_parent_class)->dispose(object);
}

static void trg_tree_view_finalize(GObject *object)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(TRG_TREE_VIEW(object));

    g_list_free_full(priv->columns, (GDestroyNotify)trg_column_description_free);
    g_free(priv->configId);

    G_OBJECT_CLASS(trg_tree_view_parent_class)->finalize(object);
}

static void trg_tree_view_class_init(TrgTreeViewClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);

    object_class->get_property = trg_tree_view_get_property;
    object_class->set_property = trg_tree_view_set_property;
    object_class->constructor = trg_tree_view_constructor;
    object_class->dispose = trg_tree_view_dispose;
    object_class->finalize = trg_tree_view_finalize;

    g_object_class_install_property(
        object_class, PROP_PREFS,
        g_param_spec_object("prefs", "Trg Prefs", "Trg Prefs", TRG_TYPE_PREFS,
                            G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    g_object_class_install_property(
        object_class, PROP_CONFIGID,
        g_param_spec_string("config-id", "config-id", "config-id", NULL,
                            G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    signals[SIGNAL_COLUMN_ADDED] = g_signal_new(
        "column-added", G_TYPE_FROM_CLASS(object_class), G_SIGNAL_RUN_LAST | G_SIGNAL_ACTION, 0,
        NULL, NULL, g_cclosure_marshal_VOID__STRING, G_TYPE_NONE, 1, G_TYPE_STRING);
}

static void trg_tree_view_init(TrgTreeView *tv)
{
    gtk_tree_view_set_rubber_banding(GTK_TREE_VIEW(tv), TRUE);
    gtk_tree_view_set_headers_clickable(GTK_TREE_VIEW(tv), TRUE);
    gtk_tree_selection_set_mode(gtk_tree_view_get_selection(GTK_TREE_VIEW(tv)),
                                GTK_SELECTION_MULTIPLE);

    gtk_widget_set_sensitive(GTK_WIDGET(tv), FALSE);
}

GtkWidget *trg_tree_view_new(void)
{
    return GTK_WIDGET(g_object_new(TRG_TYPE_TREE_VIEW, NULL));
}
