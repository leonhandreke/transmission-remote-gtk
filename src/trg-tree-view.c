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
#include <string.h>

#include "protocol-constants.h"
#include "trg-item.h"
#include "trg-prefs.h"
#include "trg-tree-view.h"
#include "util.h"

/* A GtkColumnView (in a scrolled window) of TrgItems, which allows the user
 * to change column visibility with the menu of each column header: uncheck
 * the column to hide it, or choose a hidden column to show it after.
 *
 * This class persists these choices to TrgPrefs, and restores them when it is
 * initialised. Column order, widths and the sort column are also saved and
 * restored.
 *
 * All the columns must be preregistered so it knows what model column, cell
 * widgets etc to use if it should be created, and what columns are available.
 *
 * Sorting is by a model column, which needn't be a shown column, so the view
 * has its own sorter for the GtkSortListModel and keeps the sort indicator of
 * the column view's headers in sync with it.
 */

enum {
    PROP_0,
    PROP_PREFS,
    PROP_CONFIGID
};

enum {
    SIGNAL_SORT_CHANGED,
    SIGNAL_ROW_ACTIVATED,
    SIGNAL_COUNT
};

static guint signals[SIGNAL_COUNT] = { 0 };

typedef struct {
    GList *columns;
    TrgPrefs *prefs;
    gchar *configId;

    GtkWidget *scrolled;
    GtkWidget *column_view;
    GtkWidget *list_view;

    GListModel *model;
    GtkSortListModel *sort_model;
    GtkSelectionModel *selection;
    GtkSorter *sorter;

    gint sort_column;
    GtkSortType sort_order;
    gboolean sortable;
    gboolean column_menus;
    gboolean updating_sort;

    gint search_column;
    GString *search;
    gint64 search_time;

    GSimpleActionGroup *actions;
} TrgTreeViewPrivate;

G_DEFINE_TYPE_WITH_PRIVATE(TrgTreeView, trg_tree_view, GTK_TYPE_WIDGET)

#define GDATA_KEY_COLUMN_DESC "column-desc"
#define GDATA_KEY_LIST_ITEM   "trg-list-item"
#define GDATA_KEY_CELL        "trg-cell"
#define GDATA_KEY_START_EDIT  "trg-start-editing"

/* A cell (the widgets of a column of a row) and what it's bound to */
typedef struct {
    trg_column_description *desc;
    GtkListItem *list_item;
    GtkWidget *expander;
    GtkWidget *image;
    GtkWidget *widget;
    TrgItem *item;
    gulong handler, handler_extra;
    gboolean updating;
    gchar *edit_original;
} TrgTreeViewCell;

static void trg_tree_view_update_column_menus(TrgTreeView *tv);
static void trg_tree_view_add_column_after(TrgTreeView *tv, trg_column_description *desc,
                                           gint64 width, trg_column_description *after);

gpointer trg_tree_view_row_item(gpointer row)
{
    if (GTK_IS_TREE_LIST_ROW(row)) {
        gpointer item = gtk_tree_list_row_get_item(GTK_TREE_LIST_ROW(row));
        /* the row keeps a reference */
        g_object_unref(item);
        return item;
    }

    return row;
}

gboolean trg_tree_view_is_column_showing(TrgTreeView *tv, gint index)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GList *li;

    for (li = priv->columns; li; li = g_list_next(li)) {
        trg_column_description *cd = (trg_column_description *)li->data;
        if (cd->model_column == index)
            return (cd->flags & TRG_COLUMN_SHOWING) != 0;
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

static JsonObject *trg_prefs_get_tree_view_props(TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    JsonObject *root;
    const gchar *className
        = priv->configId && strlen(priv->configId) > 0 ? priv->configId : G_OBJECT_TYPE_NAME(tv);
    JsonObject *obj;
    JsonObject *tvProps = NULL;

    if (!priv->prefs)
        return NULL;

    root = trg_prefs_get_root(priv->prefs);

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

trg_column_description *trg_tree_view_reg_column(TrgTreeView *tv, gint type, gint model_column,
                                                 const gchar *header, const gchar *id, guint flags)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    trg_column_description *desc = g_new0(trg_column_description, 1);

    desc->type = type;
    desc->model_column = model_column;
    desc->model_column_extra = -1;
    desc->header = g_strdup(header);
    desc->id = g_strdup(id);
    desc->flags = flags;
    desc->editable = (flags & TRG_COLUMN_EDITABLE) != 0;
    desc->tv = tv;

    priv->columns = g_list_append(priv->columns, desc);

    return desc;
}

static trg_column_description *trg_tree_view_find_column(TrgTreeView *tv, const gchar *id)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GList *li;

    for (li = priv->columns; li; li = g_list_next(li)) {
        trg_column_description *desc = (trg_column_description *)li->data;
        if (!g_strcmp0(desc->id, id))
            return desc;
    }

    return NULL;
}

/***
 * Value formatting, like the cell renderers of the GtkTreeView did.
 */

static gint64 trg_value_get_int64(const GValue *value)
{
    switch (G_TYPE_FUNDAMENTAL(G_VALUE_TYPE(value))) {
    case G_TYPE_INT:
        return g_value_get_int(value);
    case G_TYPE_UINT:
        return g_value_get_uint(value);
    case G_TYPE_INT64:
        return g_value_get_int64(value);
    case G_TYPE_DOUBLE:
        return (gint64)g_value_get_double(value);
    case G_TYPE_BOOLEAN:
        return g_value_get_boolean(value);
    default:
        return 0;
    }
}

static gdouble trg_value_get_double(const GValue *value)
{
    if (G_VALUE_HOLDS_DOUBLE(value))
        return g_value_get_double(value);
    return (gdouble)trg_value_get_int64(value);
}

static gchar *trg_value_to_string(const GValue *value)
{
    GValue str = G_VALUE_INIT;
    gchar *result;

    if (G_VALUE_HOLDS_STRING(value))
        return g_value_dup_string(value);

    g_value_init(&str, G_TYPE_STRING);
    if (g_value_transform(value, &str))
        result = g_value_dup_string(&str);
    else
        result = NULL;
    g_value_unset(&str);

    return result;
}

static GIcon *trg_file_icon_for(gint64 file_id, const gchar *name)
{
    if (file_id == -1)
        return g_themed_icon_new("folder");

#ifndef G_OS_WIN32
    if (name) {
        gboolean uncertain;
        gchar *mimetype = g_content_type_guess(name, NULL, 0, &uncertain);
        GIcon *icon = NULL;

        if (!uncertain && mimetype)
            icon = g_content_type_get_icon(mimetype);

        g_free(mimetype);

        if (icon)
            return icon;
    }
#endif

    return g_themed_icon_new("text-x-generic");
}

static gchar *trg_tree_view_format(trg_column_description *desc, const GValue *value)
{
    char buf[64];

    switch (desc->type) {
    case TRG_COLTYPE_SPEED: {
        gint64 speed = trg_value_get_int64(value);
        if (speed > 0)
            return g_strdup(trg_strlspeed(buf, speed / disk_K));
        return NULL;
    }
    case TRG_COLTYPE_EPOCH: {
        gint64 epoch = trg_value_get_int64(value);
        return epoch > 0 ? epoch_to_string(epoch) : NULL;
    }
    case TRG_COLTYPE_ETA: {
        gint64 eta = trg_value_get_int64(value);
        if (eta > 0)
            return g_strdup(tr_strltime_short(buf, eta, sizeof(buf)));
        else if (eta == -2)
            return g_strdup("∞");
        return NULL;
    }
    case TRG_COLTYPE_SIZE: {
        gint64 size = trg_value_get_int64(value);
        return size > 0 ? g_strdup(trg_strlsize(buf, size)) : NULL;
    }
    case TRG_COLTYPE_RATIO: {
        gdouble ratio = trg_value_get_double(value);
        return ratio > 0 ? g_strdup(trg_strlratio(buf, ratio)) : NULL;
    }
    case TRG_COLTYPE_PRIO:
        switch (trg_value_get_int64(value)) {
        case TR_PRI_LOW:
            return g_strdup(_("Low"));
        case TR_PRI_HIGH:
            return g_strdup(_("High"));
        case TR_PRI_NORMAL:
            return g_strdup(_("Normal"));
        case TR_PRI_MIXED:
            return g_strdup(_("Mixed"));
        default:
            return NULL;
        }
    case TRG_COLTYPE_NUMGTZERO:
    case TRG_COLTYPE_NUMGTEQZERO: {
        gint64 v = trg_value_get_int64(value);
        if (v >= (desc->type == TRG_COLTYPE_NUMGTZERO ? 1 : 0))
            return g_strdup_printf("%" G_GINT64_FORMAT, v);
        return NULL;
    }
    default:
        return trg_value_to_string(value);
    }
}

/***
 * Cells
 */

static void trg_tree_view_cell_refresh(TrgTreeViewCell *cell)
{
    trg_column_description *desc = cell->desc;
    TrgItem *item = cell->item;
    const GValue *value;

    if (!item)
        return;

    value = trg_item_peek_value(item, desc->model_column);
    cell->updating = TRUE;

    switch (desc->type) {
    case TRG_COLTYPE_PROG: {
        gdouble progress = CLAMP(trg_value_get_double(value), 0.0, 100.0);
        gchar *text = g_strdup_printf(C_("progress bar label", "%d %%"), (gint)progress);
        gtk_progress_bar_set_fraction(GTK_PROGRESS_BAR(cell->widget), progress / 100.0);
        gtk_progress_bar_set_text(GTK_PROGRESS_BAR(cell->widget), text);
        g_free(text);
        break;
    }
    case TRG_COLTYPE_WANTED: {
        gint64 wanted = trg_value_get_int64(value);
        gtk_check_button_set_inconsistent(GTK_CHECK_BUTTON(cell->widget), wanted == TR_PRI_MIXED);
        gtk_check_button_set_active(GTK_CHECK_BUTTON(cell->widget), wanted == TRUE);
        break;
    }
    case TRG_COLTYPE_ICONTEXT:
    case TRG_COLTYPE_FILEICONTEXT: {
        gchar *text = trg_value_to_string(value);

        gtk_label_set_text(GTK_LABEL(cell->widget), text ? text : "");

        if (desc->type == TRG_COLTYPE_ICONTEXT) {
            gtk_image_set_from_icon_name(GTK_IMAGE(cell->image),
                                         desc->model_column_extra >= 0
                                             ? trg_item_get_string(item, desc->model_column_extra)
                                             : NULL);
        } else {
            gint64 file_id = desc->model_column_extra >= 0
                ? trg_value_get_int64(trg_item_peek_value(item, desc->model_column_extra))
                : 0;
            if (file_id == -2) {
                gtk_image_clear(GTK_IMAGE(cell->image));
            } else {
                GIcon *icon = trg_file_icon_for(file_id, text);
                gtk_image_set_from_gicon(GTK_IMAGE(cell->image), icon);
                g_object_unref(icon);
            }
        }

        g_free(text);
        break;
    }
    default: {
        gchar *text = trg_tree_view_format(desc, value);

        if (GTK_IS_EDITABLE_LABEL(cell->widget)) {
            if (!gtk_editable_label_get_editing(GTK_EDITABLE_LABEL(cell->widget)))
                gtk_editable_set_text(GTK_EDITABLE(cell->widget), text ? text : "");
            gtk_widget_set_can_target(cell->widget, desc->editable);
            gtk_editable_set_editable(GTK_EDITABLE(cell->widget), desc->editable);
        } else {
            gtk_label_set_text(GTK_LABEL(cell->widget), text ? text : "");
        }

        g_free(text);
        break;
    }
    }

    cell->updating = FALSE;
}

static void trg_tree_view_cell_notify(GObject *object, GParamSpec *pspec, TrgTreeViewCell *cell)
{
    trg_tree_view_cell_refresh(cell);
}

static void trg_tree_view_wanted_toggled(GtkCheckButton *button, TrgTreeViewCell *cell)
{
    if (cell->updating || !cell->item)
        return;

    if (cell->desc->activate)
        cell->desc->activate(cell->desc->tv, cell->item, cell->desc->func_data);

    /* the model decides the state */
    trg_tree_view_cell_refresh(cell);
}

static void trg_tree_view_cell_clicked(GtkGestureClick *gesture, gint n_press, gdouble x, gdouble y,
                                       TrgTreeViewCell *cell)
{
    if (n_press == 1 && cell->item && cell->desc->activate) {
        cell->desc->activate(cell->desc->tv, cell->item, cell->desc->func_data);
        gtk_gesture_set_state(GTK_GESTURE(gesture), GTK_EVENT_SEQUENCE_CLAIMED);
    }
}

static void trg_tree_view_cell_editing_cb(GtkEditableLabel *label, GParamSpec *pspec,
                                          TrgTreeViewCell *cell)
{
    trg_column_description *desc = cell->desc;
    TrgItem *item = cell->item;

    if (!item)
        return;

    if (gtk_editable_label_get_editing(label)) {
        g_free(cell->edit_original);
        cell->edit_original = g_strdup(gtk_editable_get_text(GTK_EDITABLE(label)));
        if (desc->edit)
            desc->edit(desc->tv, item, FALSE, NULL, desc->func_data);
    } else {
        const gchar *text = gtk_editable_get_text(GTK_EDITABLE(label));
        gboolean changed = g_strcmp0(text, cell->edit_original) != 0;

        g_object_ref(item);
        if (desc->edit) {
            gchar *new_text = changed ? g_strdup(text) : NULL;
            desc->edit(desc->tv, item, TRUE, new_text, desc->func_data);
            g_free(new_text);
        }
        g_clear_pointer(&cell->edit_original, g_free);
        if (cell->item == item)
            trg_tree_view_cell_refresh(cell);
        g_object_unref(item);
    }
}

static void trg_tree_view_cell_free(TrgTreeViewCell *cell)
{
    g_free(cell->edit_original);
    g_free(cell);
}

void trg_tree_view_mark_row_widget(GtkWidget *widget, GtkListItem *list_item)
{
    g_object_set_data(G_OBJECT(widget), GDATA_KEY_LIST_ITEM, list_item);
}

static void trg_tree_view_cell_setup(GtkSignalListItemFactory *factory, GtkListItem *list_item,
                                     trg_column_description *desc)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(desc->tv);
    TrgTreeViewCell *cell = g_new0(TrgTreeViewCell, 1);
    GtkWidget *child;

    cell->desc = desc;
    cell->list_item = list_item;

    switch (desc->type) {
    case TRG_COLTYPE_ICONTEXT:
    case TRG_COLTYPE_FILEICONTEXT:
        child = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
        cell->image = gtk_image_new();
        gtk_box_append(GTK_BOX(child), cell->image);
        cell->widget = gtk_label_new(NULL);
        gtk_label_set_xalign(GTK_LABEL(cell->widget), 0);
        gtk_label_set_ellipsize(GTK_LABEL(cell->widget), PANGO_ELLIPSIZE_END);
        gtk_widget_set_hexpand(cell->widget, TRUE);
        gtk_box_append(GTK_BOX(child), cell->widget);
        break;
    case TRG_COLTYPE_WANTED:
        child = cell->widget = gtk_check_button_new();
        gtk_widget_set_halign(child, GTK_ALIGN_CENTER);
        /* the keyboard focus stays on the row, so Enter activates the row
         * rather than toggling the check box */
        gtk_widget_set_focusable(child, FALSE);
        gtk_widget_set_focus_on_click(child, FALSE);
        g_signal_connect(child, "toggled", G_CALLBACK(trg_tree_view_wanted_toggled), cell);
        break;
    case TRG_COLTYPE_PROG:
        child = cell->widget = gtk_progress_bar_new();
        gtk_progress_bar_set_show_text(GTK_PROGRESS_BAR(child), TRUE);
        gtk_widget_set_valign(child, GTK_ALIGN_CENTER);
        break;
    default:
        if (desc->flags & TRG_COLUMN_EDITABLE) {
            child = cell->widget = gtk_editable_label_new("");
            g_signal_connect(child, "notify::editing", G_CALLBACK(trg_tree_view_cell_editing_cb),
                             cell);
        } else {
            child = cell->widget = gtk_label_new(NULL);
            gtk_label_set_ellipsize(GTK_LABEL(child), PANGO_ELLIPSIZE_END);
            switch (desc->type) {
            case TRG_COLTYPE_SPEED:
            case TRG_COLTYPE_SIZE:
            case TRG_COLTYPE_RATIO:
            case TRG_COLTYPE_NUMGTZERO:
            case TRG_COLTYPE_NUMGTEQZERO:
                gtk_label_set_xalign(GTK_LABEL(child), 1);
                break;
            default:
                gtk_label_set_xalign(GTK_LABEL(child), 0);
                break;
            }
        }
        if (desc->type == TRG_COLTYPE_PRIO) {
            GtkGesture *gesture = gtk_gesture_click_new();
            gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture), GDK_BUTTON_PRIMARY);
            g_signal_connect(gesture, "released", G_CALLBACK(trg_tree_view_cell_clicked), cell);
            gtk_widget_add_controller(child, GTK_EVENT_CONTROLLER(gesture));
        }
        break;
    }

    /* the expander of a tree goes in its file name column */
    if (desc->type == TRG_COLTYPE_FILEICONTEXT && GTK_IS_TREE_LIST_MODEL(priv->model)) {
        cell->expander = gtk_tree_expander_new();
        gtk_tree_expander_set_child(GTK_TREE_EXPANDER(cell->expander), child);
        child = cell->expander;
    }

    trg_tree_view_mark_row_widget(child, list_item);
    g_object_set_data_full(G_OBJECT(list_item), GDATA_KEY_CELL, cell,
                           (GDestroyNotify)trg_tree_view_cell_free);
    gtk_list_item_set_child(list_item, child);
}

static gboolean trg_tree_view_start_editing_idle(gpointer data)
{
    GtkWidget *label = GTK_WIDGET(data);

    if (gtk_widget_get_mapped(label)) {
        gtk_widget_set_can_target(label, TRUE);
        gtk_editable_label_start_editing(GTK_EDITABLE_LABEL(label));
        gtk_widget_grab_focus(label);
    }

    g_object_unref(label);

    return G_SOURCE_REMOVE;
}

static void trg_tree_view_cell_bind(GtkSignalListItemFactory *factory, GtkListItem *list_item,
                                    trg_column_description *desc)
{
    TrgTreeViewCell *cell = g_object_get_data(G_OBJECT(list_item), GDATA_KEY_CELL);
    gpointer row = gtk_list_item_get_item(list_item);
    TrgItem *item = trg_tree_view_row_item(row);
    gchar *signal;

    if (cell->expander)
        gtk_tree_expander_set_list_row(GTK_TREE_EXPANDER(cell->expander),
                                       GTK_IS_TREE_LIST_ROW(row) ? row : NULL);

    cell->item = item;

    signal = g_strconcat(
        "notify::", g_param_spec_get_name(trg_item_get_column_pspec(item, desc->model_column)),
        NULL);
    cell->handler = g_signal_connect(item, signal, G_CALLBACK(trg_tree_view_cell_notify), cell);
    g_free(signal);

    if ((desc->type == TRG_COLTYPE_ICONTEXT || desc->type == TRG_COLTYPE_FILEICONTEXT)
        && desc->model_column_extra >= 0) {
        signal = g_strconcat(
            "notify::",
            g_param_spec_get_name(trg_item_get_column_pspec(item, desc->model_column_extra)), NULL);
        cell->handler_extra
            = g_signal_connect(item, signal, G_CALLBACK(trg_tree_view_cell_notify), cell);
        g_free(signal);
    }

    trg_tree_view_cell_refresh(cell);

    if (GTK_IS_EDITABLE_LABEL(cell->widget)
        && g_object_get_data(G_OBJECT(item), GDATA_KEY_START_EDIT) == desc) {
        g_object_set_data(G_OBJECT(item), GDATA_KEY_START_EDIT, NULL);
        g_idle_add(trg_tree_view_start_editing_idle, g_object_ref(cell->widget));
    }
}

static void trg_tree_view_cell_unbind(GtkSignalListItemFactory *factory, GtkListItem *list_item,
                                      trg_column_description *desc)
{
    TrgTreeViewCell *cell = g_object_get_data(G_OBJECT(list_item), GDATA_KEY_CELL);

    if (cell->item) {
        g_clear_signal_handler(&cell->handler, cell->item);
        g_clear_signal_handler(&cell->handler_extra, cell->item);
    }

    if (cell->expander)
        gtk_tree_expander_set_list_row(GTK_TREE_EXPANDER(cell->expander), NULL);

    cell->item = NULL;
}

void trg_tree_view_set_column_editable(trg_column_description *desc, gboolean editable)
{
    desc->editable = editable;
}

static guint trg_tree_view_find_position(TrgTreeView *tv, gpointer item)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GListModel *model = G_LIST_MODEL(priv->selection);
    guint i, n = model ? g_list_model_get_n_items(model) : 0;

    for (i = 0; i < n; i++) {
        g_autoptr(GObject) row = g_list_model_get_item(model, i);
        if (trg_tree_view_row_item(row) == item)
            return i;
    }

    return GTK_INVALID_LIST_POSITION;
}

static GtkWidget *trg_tree_view_get_list_base(TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GtkWidget *child;

    if (priv->list_view && gtk_widget_get_parent(priv->list_view))
        return priv->list_view;

    /* the rows of a column view are in a GtkListView */
    for (child = gtk_widget_get_first_child(priv->column_view); child;
         child = gtk_widget_get_next_sibling(child)) {
        if (GTK_IS_LIST_VIEW(child))
            return child;
    }

    return NULL;
}

static void trg_tree_view_scroll_to(TrgTreeView *tv, guint position)
{
    GtkWidget *list = trg_tree_view_get_list_base(tv);

    if (list && position != GTK_INVALID_LIST_POSITION)
        gtk_widget_activate_action(list, "list.scroll-to-item", "u", position);
}

/* Selects only the row at position, and moves the keyboard focus to it, so
 * the cursor keys go on from there (for the type ahead search, which has the
 * focus already). */
static void trg_tree_view_select_and_focus(TrgTreeView *tv, guint position)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);

#if GTK_CHECK_VERSION(4, 12, 0)
    GtkListScrollFlags flags = GTK_LIST_SCROLL_FOCUS | GTK_LIST_SCROLL_SELECT;

    if (priv->list_view && gtk_widget_get_parent(priv->list_view))
        gtk_list_view_scroll_to(GTK_LIST_VIEW(priv->list_view), position, flags, NULL);
    else
        gtk_column_view_scroll_to(GTK_COLUMN_VIEW(priv->column_view), position, NULL, flags, NULL);
#else
    /* this doesn't move the focus */
    gtk_selection_model_select_item(priv->selection, position, TRUE);
    trg_tree_view_scroll_to(tv, position);
#endif
}

/* Starts editing the cell of item in the editable column desc, once it's
 * shown. */
void trg_tree_view_start_editing(TrgTreeView *tv, gpointer item, trg_column_description *desc)
{
    g_object_set_data(G_OBJECT(item), GDATA_KEY_START_EDIT, desc);
    trg_tree_view_scroll_to(tv, trg_tree_view_find_position(tv, item));
}

/***
 * Sorting
 */

static int trg_tree_view_sort_func(gconstpointer a, gconstpointer b, gpointer data)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(TRG_TREE_VIEW(data));
    gint column = priv->sort_column;
    gint result;

    if (column < 0 || (guint)column >= trg_item_get_n_columns((gpointer)a))
        return GTK_ORDERING_EQUAL;

    result = trg_item_compare((gpointer)a, (gpointer)b, column);
    if (priv->sort_order == GTK_SORT_DESCENDING)
        result = -result;

    return result < 0 ? GTK_ORDERING_SMALLER
                      : (result > 0 ? GTK_ORDERING_LARGER : GTK_ORDERING_EQUAL);
}

static trg_column_description *trg_tree_view_find_model_column(TrgTreeView *tv, gint column)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GList *li;

    for (li = priv->columns; li; li = g_list_next(li)) {
        trg_column_description *desc = (trg_column_description *)li->data;
        if (desc->model_column == column)
            return desc;
    }

    return NULL;
}

/* Show the sort column in the column headers */
static void trg_tree_view_sync_sort_indicator(TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    trg_column_description *desc = trg_tree_view_find_model_column(tv, priv->sort_column);

    if (!priv->column_view)
        return;

    priv->updating_sort = TRUE;
    gtk_column_view_sort_by_column(GTK_COLUMN_VIEW(priv->column_view), desc ? desc->column : NULL,
                                   priv->sort_order);
    priv->updating_sort = FALSE;
}

void trg_tree_view_set_sort(TrgTreeView *tv, gint column, GtkSortType order)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);

    if (column < 0)
        column = -1;

    if (priv->sort_column == column && priv->sort_order == order)
        return;

    priv->sort_column = column;
    priv->sort_order = order;

    trg_tree_view_sync_sort_indicator(tv);
    trg_tree_view_resort(tv);

    g_signal_emit(tv, signals[SIGNAL_SORT_CHANGED], 0);
}

gint trg_tree_view_get_sort(TrgTreeView *tv, GtkSortType *order)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);

    if (order)
        *order = priv->sort_order;

    return priv->sort_column;
}

/* The items changed, so they may be in a different order now */
void trg_tree_view_resort(TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);

    if (priv->sort_column >= 0)
        gtk_sorter_changed(priv->sorter, GTK_SORTER_CHANGE_DIFFERENT);
}

/* A column header was clicked */
static void trg_tree_view_column_sorter_changed(GtkSorter *sorter, GtkSorterChange change,
                                                TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GtkColumnViewSorter *cvs = GTK_COLUMN_VIEW_SORTER(sorter);
    GtkColumnViewColumn *column;
    trg_column_description *desc;

    if (priv->updating_sort)
        return;

    column = gtk_column_view_sorter_get_primary_sort_column(cvs);
    desc = column ? g_object_get_data(G_OBJECT(column), GDATA_KEY_COLUMN_DESC) : NULL;

    if (desc)
        trg_tree_view_set_sort(tv, desc->model_column,
                               gtk_column_view_sorter_get_primary_sort_order(cvs));
    else
        trg_tree_view_set_sort(tv, -1, GTK_SORT_ASCENDING);
}

void trg_tree_view_set_sortable(TrgTreeView *tv, gboolean sortable)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);

    priv->sortable = sortable;
}

static void trg_tree_view_sort_type_change_state(GSimpleAction *action, GVariant *value,
                                                 gpointer data)
{
    TrgTreeView *tv = TRG_TREE_VIEW(data);

    trg_tree_view_set_sort(tv, trg_tree_view_get_sort(tv, NULL), g_variant_get_int32(value));
}

static void trg_tree_view_sort_column_change_state(GSimpleAction *action, GVariant *value,
                                                   gpointer data)
{
    TrgTreeView *tv = TRG_TREE_VIEW(data);
    GtkSortType sortType;

    trg_tree_view_get_sort(tv, &sortType);
    trg_tree_view_set_sort(tv, g_variant_get_int32(value), sortType);
}

/* Keep the radio items in sync when sorting changes elsewhere, eg. by
 * clicking a column header. */
static void trg_tree_view_sort_changed_cb(TrgTreeView *tv, gpointer data)
{
    GActionMap *map = G_ACTION_MAP(data);
    GtkSortType sortType;
    gint sort = trg_tree_view_get_sort(tv, &sortType);

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
    GMenu *menu = g_menu_new();
    GMenu *section;
    GMenuItem *item;
    GSimpleAction *action;
    GList *li;
    GtkSortType sortType;
    gint sort = trg_tree_view_get_sort(tv, &sortType);

    action = g_simple_action_new_stateful(TRG_TREE_VIEW_SORT_TYPE_ACTION, G_VARIANT_TYPE_INT32,
                                          g_variant_new_int32(sortType));
    g_signal_connect(action, "change-state", G_CALLBACK(trg_tree_view_sort_type_change_state), tv);
    g_action_map_add_action(map, G_ACTION(action));
    g_object_unref(action);

    action = g_simple_action_new_stateful(TRG_TREE_VIEW_SORT_COLUMN_ACTION, G_VARIANT_TYPE_INT32,
                                          g_variant_new_int32(sort));
    g_signal_connect(action, "change-state", G_CALLBACK(trg_tree_view_sort_column_change_state),
                     tv);
    g_action_map_add_action(map, G_ACTION(action));
    g_object_unref(action);

    g_signal_connect_object(tv, "sort-changed", G_CALLBACK(trg_tree_view_sort_changed_cb), map, 0);

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

/***
 * Columns
 */

static guint trg_tree_view_count_showing(TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    guint n = 0;
    GList *li;

    for (li = priv->columns; li; li = g_list_next(li))
        if (((trg_column_description *)li->data)->flags & TRG_COLUMN_SHOWING)
            n++;

    return n;
}

static void trg_tree_view_hide_column(TrgTreeView *tv, trg_column_description *desc)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);

    if (!desc->column)
        return;

    gtk_column_view_remove_column(GTK_COLUMN_VIEW(priv->column_view), desc->column);
    g_clear_object(&desc->column);
    desc->flags &= ~TRG_COLUMN_SHOWING;
}

static void trg_tree_view_column_visible_activate(GSimpleAction *action, GVariant *parameter,
                                                  gpointer data)
{
    TrgTreeView *tv = TRG_TREE_VIEW(data);
    trg_column_description *desc = g_object_get_data(G_OBJECT(action), GDATA_KEY_COLUMN_DESC);

    if (desc->flags & TRG_COLUMN_SHOWING) {
        trg_tree_view_hide_column(tv, desc);
        trg_tree_view_sync_sort_indicator(tv);
        trg_tree_view_update_column_menus(tv);
    }
}

static void trg_tree_view_column_show_after_activate(GSimpleAction *action, GVariant *parameter,
                                                     gpointer data)
{
    TrgTreeView *tv = TRG_TREE_VIEW(data);
    const gchar *id, *after_id;
    trg_column_description *desc, *after;

    g_variant_get(parameter, "(&s&s)", &id, &after_id);
    desc = trg_tree_view_find_column(tv, id);
    after = trg_tree_view_find_column(tv, after_id);

    if (desc && !(desc->flags & TRG_COLUMN_SHOWING)) {
        trg_tree_view_add_column_after(tv, desc, -1, after);
        trg_tree_view_sync_sort_indicator(tv);
        trg_tree_view_update_column_menus(tv);
    }
}

static GSimpleAction *trg_tree_view_column_action(TrgTreeView *tv, trg_column_description *desc)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    gchar *name = g_strconcat("visible-", desc->id, NULL);
    GAction *action = g_action_map_lookup_action(G_ACTION_MAP(priv->actions), name);

    if (!action) {
        action = G_ACTION(g_simple_action_new_stateful(name, NULL, g_variant_new_boolean(FALSE)));
        g_object_set_data(G_OBJECT(action), GDATA_KEY_COLUMN_DESC, desc);
        g_signal_connect(action, "activate", G_CALLBACK(trg_tree_view_column_visible_activate), tv);
        g_action_map_add_action(G_ACTION_MAP(priv->actions), action);
        g_object_unref(action);
    }

    g_free(name);

    return G_SIMPLE_ACTION(action);
}

/* The menu of each column header has the column itself, checked, which hides
 * it, and then the hidden columns, which show themselves after it. */
static void trg_tree_view_update_column_menus(TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    guint n_showing = trg_tree_view_count_showing(tv);
    GList *li, *lj;

    for (li = priv->columns; li; li = g_list_next(li)) {
        trg_column_description *desc = (trg_column_description *)li->data;
        GSimpleAction *action = trg_tree_view_column_action(tv, desc);
        gboolean showing = (desc->flags & TRG_COLUMN_SHOWING) != 0;
        GMenu *menu;
        gchar *detailed;

        g_simple_action_set_state(action, g_variant_new_boolean(showing));
        g_simple_action_set_enabled(
            action, showing && !(desc->flags & TRG_COLUMN_UNREMOVABLE) && n_showing > 1);

        if (!desc->column)
            continue;

        if (!priv->column_menus) {
            gtk_column_view_column_set_header_menu(desc->column, NULL);
            continue;
        }

        menu = g_menu_new();
        detailed = g_strconcat("tvcol.visible-", desc->id, NULL);
        g_menu_append(menu, desc->header, detailed);
        g_free(detailed);

        for (lj = priv->columns; lj; lj = g_list_next(lj)) {
            trg_column_description *hidden = (trg_column_description *)lj->data;
            if (!(hidden->flags & TRG_COLUMN_SHOWING)) {
                GMenuItem *item = g_menu_item_new(hidden->header, NULL);
                g_menu_item_set_action_and_target(item, "tvcol.show-after", "(ss)", hidden->id,
                                                  desc->id);
                g_menu_append_item(menu, item);
                g_object_unref(item);
            }
        }

        gtk_column_view_column_set_header_menu(desc->column, G_MENU_MODEL(menu));
        g_object_unref(menu);
    }
}

void trg_tree_view_set_column_menus(TrgTreeView *tv, gboolean enabled)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);

    priv->column_menus = enabled;
    trg_tree_view_update_column_menus(tv);
}

static void trg_tree_view_add_column_after(TrgTreeView *tv, trg_column_description *desc,
                                           gint64 width, trg_column_description *after)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GtkColumnView *cv = GTK_COLUMN_VIEW(priv->column_view);
    GtkListItemFactory *factory = gtk_signal_list_item_factory_new();
    GtkColumnViewColumn *column;
    guint position = g_list_model_get_n_items(gtk_column_view_get_columns(cv));

    g_signal_connect(factory, "setup", G_CALLBACK(trg_tree_view_cell_setup), desc);
    g_signal_connect(factory, "bind", G_CALLBACK(trg_tree_view_cell_bind), desc);
    g_signal_connect(factory, "unbind", G_CALLBACK(trg_tree_view_cell_unbind), desc);

    column = gtk_column_view_column_new(desc->header, factory);
    gtk_column_view_column_set_id(column, desc->id);
    gtk_column_view_column_set_resizable(column, TRUE);
    gtk_column_view_column_set_expand(column, TRUE);

    if (width > 0)
        gtk_column_view_column_set_fixed_width(column, (gint)width);

    /* The column view's sorter only shows the sort column (and makes the
     * headers clickable), the view's own sorter sorts. */
    if (priv->sortable) {
        GtkSorter *sorter = GTK_SORTER(gtk_custom_sorter_new(NULL, NULL, NULL));
        gtk_column_view_column_set_sorter(column, sorter);
        g_object_unref(sorter);
    }

    g_object_set_data(G_OBJECT(column), GDATA_KEY_COLUMN_DESC, desc);

    if (after && after->column) {
        GListModel *columns = gtk_column_view_get_columns(cv);
        guint i, n = g_list_model_get_n_items(columns);
        for (i = 0; i < n; i++) {
            g_autoptr(GtkColumnViewColumn) c = g_list_model_get_item(columns, i);
            if (c == after->column) {
                position = i + 1;
                break;
            }
        }
    }

    gtk_column_view_insert_column(cv, position, column);

    desc->column = column;
    desc->flags |= TRG_COLUMN_SHOWING;
}

void trg_tree_view_remove_all_columns(TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GList *li;

    for (li = priv->columns; li; li = g_list_next(li))
        trg_tree_view_hide_column(tv, (trg_column_description *)li->data);
}

void trg_tree_view_persist(TrgTreeView *tv, guint flags)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    JsonObject *props = trg_prefs_get_tree_view_props(tv);
    JsonArray *widths, *columns;
    GtkSortType sort_type;
    gint sort_column_id;

    if (!props)
        return;

    if (flags & TRG_TREE_VIEW_PERSIST_SORT) {
        sort_column_id = trg_tree_view_get_sort(tv, &sort_type);

        if (json_object_has_member(props, TRG_PREFS_KEY_TV_SORT_COL))
            json_object_remove_member(props, TRG_PREFS_KEY_TV_SORT_COL);

        if (json_object_has_member(props, TRG_PREFS_KEY_TV_SORT_TYPE))
            json_object_remove_member(props, TRG_PREFS_KEY_TV_SORT_TYPE);

        json_object_set_int_member(props, TRG_PREFS_KEY_TV_SORT_COL, (gint64)sort_column_id);
        json_object_set_int_member(props, TRG_PREFS_KEY_TV_SORT_TYPE, (gint64)sort_type);
    }

    if (flags & TRG_TREE_VIEW_PERSIST_LAYOUT) {
        GListModel *cols = gtk_column_view_get_columns(GTK_COLUMN_VIEW(priv->column_view));
        guint i, n = g_list_model_get_n_items(cols);

        if (json_object_has_member(props, TRG_PREFS_KEY_TV_WIDTHS))
            json_object_remove_member(props, TRG_PREFS_KEY_TV_WIDTHS);

        widths = json_array_new();
        json_object_set_array_member(props, TRG_PREFS_KEY_TV_WIDTHS, widths);

        if (json_object_has_member(props, TRG_PREFS_KEY_TV_COLUMNS))
            json_object_remove_member(props, TRG_PREFS_KEY_TV_COLUMNS);

        columns = json_array_new();
        json_object_set_array_member(props, TRG_PREFS_KEY_TV_COLUMNS, columns);

        /* The width is the one the user resized the column to, or -1 */
        for (i = 0; i < n; i++) {
            g_autoptr(GtkColumnViewColumn) col = g_list_model_get_item(cols, i);
            trg_column_description *desc = g_object_get_data(G_OBJECT(col), GDATA_KEY_COLUMN_DESC);

            json_array_add_string_element(columns, desc->id);
            json_array_add_int_element(widths, gtk_column_view_column_get_fixed_width(col));
        }
    }
}

void trg_tree_view_restore_sort(TrgTreeView *tv)
{
    JsonObject *props = trg_prefs_get_tree_view_props(tv);

    if (props && json_object_has_member(props, TRG_PREFS_KEY_TV_SORT_COL)
        && json_object_has_member(props, TRG_PREFS_KEY_TV_SORT_TYPE)) {
        gint64 sort_col = json_object_get_int_member(props, TRG_PREFS_KEY_TV_SORT_COL);
        gint64 sort_type = json_object_get_int_member(props, TRG_PREFS_KEY_TV_SORT_TYPE);
        trg_tree_view_set_sort(tv, (gint)sort_col, (GtkSortType)sort_type);
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

    if (!props || !json_object_has_member(props, TRG_PREFS_KEY_TV_COLUMNS)
        || !json_object_has_member(props, TRG_PREFS_KEY_TV_WIDTHS)) {
        GList *li;
        for (li = priv->columns; li; li = g_list_next(li)) {
            trg_column_description *desc = (trg_column_description *)li->data;
            if (desc && !(desc->flags & TRG_COLUMN_EXTRA))
                trg_tree_view_add_column_after(tv, desc, -1, NULL);
        }
    } else {
        columns = json_array_get_elements(
            json_object_get_array_member(props, TRG_PREFS_KEY_TV_COLUMNS));
        widths
            = json_array_get_elements(json_object_get_array_member(props, TRG_PREFS_KEY_TV_WIDTHS));

        for (cli = columns, wli = widths; cli && wli;
             cli = g_list_next(cli), wli = g_list_next(wli)) {
            trg_column_description *desc
                = trg_tree_view_find_column(tv, json_node_get_string((JsonNode *)cli->data));
            if (desc && !(desc->flags & TRG_COLUMN_SHOWING))
                trg_tree_view_add_column_after(tv, desc, json_node_get_int((JsonNode *)wli->data),
                                               NULL);
        }

        g_list_free(columns);
        g_list_free(widths);
    }

    trg_tree_view_sync_sort_indicator(tv);
    trg_tree_view_update_column_menus(tv);
}

/***
 * Model and selection
 */

void trg_tree_view_set_model(TrgTreeView *tv, GListModel *model)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GtkSorter *sorter;

    g_set_object(&priv->model, model);

    if (GTK_IS_TREE_LIST_MODEL(model))
        sorter = GTK_SORTER(gtk_tree_list_row_sorter_new(g_object_ref(priv->sorter)));
    else
        sorter = g_object_ref(priv->sorter);

    g_clear_object(&priv->sort_model);
    priv->sort_model = gtk_sort_list_model_new(g_object_ref(model), sorter);
    gtk_multi_selection_set_model(GTK_MULTI_SELECTION(priv->selection),
                                  G_LIST_MODEL(priv->sort_model));
}

GListModel *trg_tree_view_get_model(TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);

    return priv->model;
}

GtkSelectionModel *trg_tree_view_get_selection(TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);

    return priv->selection;
}

/* The selected items, in the view's order. The list doesn't hold references. */
GList *trg_tree_view_get_selected_items(TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GtkBitset *selected = gtk_selection_model_get_selection(priv->selection);
    GtkBitsetIter iter;
    GList *items = NULL;
    guint position;

    if (gtk_bitset_iter_init_first(&iter, selected, &position)) {
        do {
            GObject *row = g_list_model_get_item(G_LIST_MODEL(priv->selection), position);
            items = g_list_prepend(items, trg_tree_view_row_item(row));
            g_object_unref(row);
        } while (gtk_bitset_iter_next(&iter, &position));
    }

    gtk_bitset_unref(selected);

    return g_list_reverse(items);
}

guint trg_tree_view_count_selected(TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GtkBitset *selected = gtk_selection_model_get_selection(priv->selection);
    guint n = (guint)gtk_bitset_get_size(selected);

    gtk_bitset_unref(selected);

    return n;
}

gpointer trg_tree_view_get_first_selected(TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GtkBitset *selected = gtk_selection_model_get_selection(priv->selection);
    gpointer item = NULL;

    if (!gtk_bitset_is_empty(selected)) {
        GObject *row = g_list_model_get_item(G_LIST_MODEL(priv->selection),
                                             gtk_bitset_get_minimum(selected));
        item = trg_tree_view_row_item(row);
        g_object_unref(row);
    }

    gtk_bitset_unref(selected);

    return item;
}

void trg_tree_view_select_item(TrgTreeView *tv, gpointer item)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    guint position = trg_tree_view_find_position(tv, item);

    if (position != GTK_INVALID_LIST_POSITION) {
        gtk_selection_model_select_item(priv->selection, position, TRUE);
        trg_tree_view_scroll_to(tv, position);
    }
}

static GtkListItem *trg_tree_view_find_marked(GtkWidget *widget);

static GtkListItem *trg_tree_view_list_item_for(GtkWidget *widget, GtkWidget *stop)
{
    for (; widget && widget != stop; widget = gtk_widget_get_parent(widget)) {
        GtkListItem *list_item = g_object_get_data(G_OBJECT(widget), GDATA_KEY_LIST_ITEM);
        GtkWidget *parent = gtk_widget_get_parent(widget);

        if (list_item)
            return list_item;

        /* the padding of a row (a child of the list view) is outside its
         * row widget or cells */
        if (parent && GTK_IS_LIST_VIEW(parent))
            return trg_tree_view_find_marked(widget);
    }

    return NULL;
}

gboolean trg_tree_view_point_in_header(TrgTreeView *tv, gdouble x, gdouble y)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GtkWidget *widget = gtk_widget_pick(GTK_WIDGET(tv), x, y, GTK_PICK_DEFAULT);

    for (; widget && widget != priv->column_view; widget = gtk_widget_get_parent(widget)) {
        if (!g_strcmp0(gtk_widget_get_css_name(widget), "header")
            && gtk_widget_get_parent(widget) == priv->column_view)
            return TRUE;
    }

    return FALSE;
}

/* The item of the row at x, y (in the view's coordinates) */
gpointer trg_tree_view_get_item_at(TrgTreeView *tv, gdouble x, gdouble y, guint *position)
{
    /* cells may not be targetable, like a text cell that can't be edited */
    GtkWidget *widget
        = gtk_widget_pick(GTK_WIDGET(tv), x, y, GTK_PICK_INSENSITIVE | GTK_PICK_NON_TARGETABLE);
    GtkListItem *list_item = trg_tree_view_list_item_for(widget, GTK_WIDGET(tv));
    gpointer row;

    if (!list_item || !(row = gtk_list_item_get_item(list_item)))
        return NULL;

    if (position)
        *position = gtk_list_item_get_position(list_item);

    return trg_tree_view_row_item(row);
}

/* For a context menu: select the row at x, y unless it's already selected.
 * Returns whether there was a row. */
gboolean trg_tree_view_select_at(TrgTreeView *tv, gdouble x, gdouble y)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    guint position;

    if (!trg_tree_view_get_item_at(tv, x, y, &position))
        return FALSE;

    if (!gtk_selection_model_is_selected(priv->selection, position))
        gtk_selection_model_select_item(priv->selection, position, TRUE);

    return TRUE;
}

static GtkListItem *trg_tree_view_find_marked(GtkWidget *widget)
{
    GtkListItem *list_item = g_object_get_data(G_OBJECT(widget), GDATA_KEY_LIST_ITEM);
    GtkWidget *child;

    for (child = gtk_widget_get_first_child(widget); child && !list_item;
         child = gtk_widget_get_next_sibling(child))
        list_item = trg_tree_view_find_marked(child);

    return list_item;
}

/* The bounds of the row with the keyboard focus, for a context menu opened
 * from the keyboard. */
gboolean trg_tree_view_get_focus_rect(TrgTreeView *tv, graphene_rect_t *rect)
{
    GtkRoot *root = gtk_widget_get_root(GTK_WIDGET(tv));
    GtkWidget *focus = root ? gtk_root_get_focus(root) : NULL;

    if (!focus || !gtk_widget_is_ancestor(focus, GTK_WIDGET(tv)))
        return FALSE;

    /* the focus is on the row widget, or in one of its cells */
    if (!trg_tree_view_find_marked(focus)) {
        GtkListItem *list_item = trg_tree_view_list_item_for(focus, GTK_WIDGET(tv));
        if (!list_item)
            return FALSE;
    }

    return gtk_widget_compute_bounds(focus, GTK_WIDGET(tv), rect);
}

void trg_tree_view_expand_all(TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GtkTreeListModel *model;
    guint i;

    if (!GTK_IS_TREE_LIST_MODEL(priv->model))
        return;

    model = GTK_TREE_LIST_MODEL(priv->model);

    /* expanding inserts the children after the row, so they're seen too */
    for (i = 0; i < g_list_model_get_n_items(G_LIST_MODEL(model)); i++) {
        GtkTreeListRow *row = gtk_tree_list_model_get_row(model, i);
        if (row) {
            gtk_tree_list_row_set_expanded(row, TRUE);
            g_object_unref(row);
        }
    }
}

void trg_tree_view_collapse_all(TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GtkTreeListModel *model;
    guint i;

    if (!GTK_IS_TREE_LIST_MODEL(priv->model))
        return;

    model = GTK_TREE_LIST_MODEL(priv->model);

    for (i = 0; i < g_list_model_get_n_items(G_LIST_MODEL(model)); i++) {
        GtkTreeListRow *row = gtk_tree_list_model_get_row(model, i);
        if (row) {
            gtk_tree_list_row_set_expanded(row, FALSE);
            g_object_unref(row);
        }
    }
}

/***
 * Type ahead search, like GtkTreeView's: typing selects the first row whose
 * search column starts with (or else contains) the typed text.
 */

void trg_tree_view_set_search_column(TrgTreeView *tv, gint column)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);

    priv->search_column = column;
}

static gboolean trg_tree_view_search(TrgTreeView *tv, const gchar *key)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GListModel *model = G_LIST_MODEL(priv->selection);
    guint i, n = g_list_model_get_n_items(model);
    guint found = GTK_INVALID_LIST_POSITION;
    gchar *needle = g_utf8_casefold(key, -1);
    gint pass;

    for (pass = 0; pass < 2 && found == GTK_INVALID_LIST_POSITION; pass++) {
        for (i = 0; i < n && found == GTK_INVALID_LIST_POSITION; i++) {
            g_autoptr(GObject) row = g_list_model_get_item(model, i);
            TrgItem *item = trg_tree_view_row_item(row);
            gchar *text = trg_value_to_string(trg_item_peek_value(item, priv->search_column));
            gchar *folded = text ? g_utf8_casefold(text, -1) : NULL;

            if (folded
                && (pass == 0 ? g_str_has_prefix(folded, needle) : strstr(folded, needle) != NULL))
                found = i;

            g_free(folded);
            g_free(text);
        }
    }

    g_free(needle);

    if (found != GTK_INVALID_LIST_POSITION) {
        trg_tree_view_select_and_focus(tv, found);
        return TRUE;
    }

    return FALSE;
}

/* The row (a GtkTreeListRow of a tree) with the keyboard focus */
static gpointer trg_tree_view_get_focus_row(TrgTreeView *tv)
{
    GtkRoot *root = gtk_widget_get_root(GTK_WIDGET(tv));
    GtkWidget *focus = root ? gtk_root_get_focus(root) : NULL;
    GtkListItem *list_item;

    if (!focus || !gtk_widget_is_ancestor(focus, GTK_WIDGET(tv)))
        return NULL;

    /* the focus is on the row widget, or in one of its cells */
    list_item = trg_tree_view_find_marked(focus);
    if (!list_item)
        list_item = trg_tree_view_list_item_for(focus, GTK_WIDGET(tv));

    return list_item ? gtk_list_item_get_item(list_item) : NULL;
}

static void trg_tree_view_expand_recursive(GtkTreeListRow *row)
{
    GListModel *children;
    guint i, n;

    gtk_tree_list_row_set_expanded(row, TRUE);

    children = gtk_tree_list_row_get_children(row);
    n = children ? g_list_model_get_n_items(children) : 0;

    for (i = 0; i < n; i++) {
        GtkTreeListRow *child = gtk_tree_list_row_get_child_row(row, i);
        if (child) {
            trg_tree_view_expand_recursive(child);
            g_object_unref(child);
        }
    }
}

/* The keys of GtkTreeView (and GtkTreeExpander) to expand or collapse the
 * row with the focus: + and Shift+Right expand, * expands all below, - and
 * / and Shift+Left collapse. */
static gboolean trg_tree_view_expand_key(TrgTreeView *tv, guint keyval, GdkModifierType state)
{
    GtkTreeListRow *row;
    gboolean shift = (state & GDK_SHIFT_MASK) != 0;

    row = trg_tree_view_get_focus_row(tv);
    if (!GTK_IS_TREE_LIST_ROW(row) || !gtk_tree_list_row_is_expandable(row))
        return FALSE;

    switch (keyval) {
    case GDK_KEY_plus:
    case GDK_KEY_KP_Add:
        gtk_tree_list_row_set_expanded(row, TRUE);
        return TRUE;
    case GDK_KEY_asterisk:
    case GDK_KEY_KP_Multiply:
        trg_tree_view_expand_recursive(row);
        return TRUE;
    case GDK_KEY_minus:
    case GDK_KEY_KP_Subtract:
    case GDK_KEY_slash:
    case GDK_KEY_KP_Divide:
        gtk_tree_list_row_set_expanded(row, FALSE);
        return TRUE;
    case GDK_KEY_Right:
    case GDK_KEY_KP_Right:
        if (shift)
            gtk_tree_list_row_set_expanded(row, TRUE);
        return shift;
    case GDK_KEY_Left:
    case GDK_KEY_KP_Left:
        if (shift)
            gtk_tree_list_row_set_expanded(row, FALSE);
        return shift;
    default:
        return FALSE;
    }
}

/* Before the list's own keys (Shift+arrows extend the selection) */
static gboolean trg_tree_view_expand_key_pressed(GtkEventControllerKey *controller, guint keyval,
                                                 guint keycode, GdkModifierType state,
                                                 TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    gboolean searching
        = priv->search->len > 0 && g_get_monotonic_time() - priv->search_time <= G_USEC_PER_SEC;

    /* unless they're part of a search */
    if (searching || !GTK_IS_TREE_LIST_MODEL(priv->model)
        || (state & (GDK_CONTROL_MASK | GDK_ALT_MASK | GDK_META_MASK | GDK_SUPER_MASK)))
        return FALSE;

    return trg_tree_view_expand_key(tv, keyval, state);
}

static gboolean trg_tree_view_key_pressed(GtkEventControllerKey *controller, guint keyval,
                                          guint keycode, GdkModifierType state, TrgTreeView *tv)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    gunichar c = gdk_keyval_to_unicode(keyval);
    gint64 now = g_get_monotonic_time();

    if (priv->search_column < 0 || !c || !g_unichar_isprint(c)
        || (state & (GDK_CONTROL_MASK | GDK_ALT_MASK | GDK_META_MASK | GDK_SUPER_MASK)))
        return FALSE;

    /* a pause starts a new search */
    if (now - priv->search_time > G_USEC_PER_SEC)
        g_string_truncate(priv->search, 0);
    priv->search_time = now;

    g_string_append_unichar(priv->search, c);

    trg_tree_view_search(tv, priv->search->str);

    return TRUE;
}

/***
 * Showing the rows in a GtkListView
 */

static void trg_tree_view_activate_cb(GtkWidget *view, guint position, TrgTreeView *tv)
{
    g_signal_emit(tv, signals[SIGNAL_ROW_ACTIVATED], 0, position);
}

void trg_tree_view_set_list_factory(TrgTreeView *tv, GtkListItemFactory *factory)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);

    if (factory) {
        if (!priv->list_view) {
            priv->list_view = g_object_ref_sink(gtk_list_view_new(NULL, NULL));
            g_signal_connect(priv->list_view, "activate", G_CALLBACK(trg_tree_view_activate_cb),
                             tv);
        }
        gtk_list_view_set_factory(GTK_LIST_VIEW(priv->list_view), factory);
        gtk_list_view_set_model(GTK_LIST_VIEW(priv->list_view), priv->selection);
        gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(priv->scrolled), priv->list_view);
    } else {
        if (priv->list_view) {
            gtk_list_view_set_model(GTK_LIST_VIEW(priv->list_view), NULL);
            gtk_list_view_set_factory(GTK_LIST_VIEW(priv->list_view), NULL);
        }
        gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(priv->scrolled), priv->column_view);
    }
}

/***
 * GtkWidget
 */

static void trg_tree_view_measure(GtkWidget *widget, GtkOrientation orientation, int for_size,
                                  int *minimum, int *natural, int *minimum_baseline,
                                  int *natural_baseline)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(TRG_TREE_VIEW(widget));

    gtk_widget_measure(priv->scrolled, orientation, for_size, minimum, natural, minimum_baseline,
                       natural_baseline);
}

/* The context menus are popovers of the view */
static void trg_tree_view_size_allocate(GtkWidget *widget, int width, int height, int baseline)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(TRG_TREE_VIEW(widget));
    GtkWidget *child;

    gtk_widget_allocate(priv->scrolled, width, height, baseline, NULL);

    for (child = gtk_widget_get_first_child(widget); child;
         child = gtk_widget_get_next_sibling(child)) {
        if (GTK_IS_POPOVER(child))
            gtk_popover_present(GTK_POPOVER(child));
    }
}

static gboolean trg_tree_view_focus(GtkWidget *widget, GtkDirectionType direction)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(TRG_TREE_VIEW(widget));

    return gtk_widget_child_focus(priv->scrolled, direction);
}

static gboolean trg_tree_view_grab_focus(GtkWidget *widget)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(TRG_TREE_VIEW(widget));

    return gtk_widget_grab_focus(
        gtk_scrolled_window_get_child(GTK_SCROLLED_WINDOW(priv->scrolled)));
}

static void trg_tree_view_dispose(GObject *object)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(TRG_TREE_VIEW(object));
    GtkWidget *child;
    GList *li;

    /* destroying the column view changes its sorter, which isn't a choice of
     * the user's to sort by */
    if (priv->column_view)
        g_signal_handlers_disconnect_by_data(
            gtk_column_view_get_sorter(GTK_COLUMN_VIEW(priv->column_view)), object);

    for (li = priv->columns; li; li = g_list_next(li))
        g_clear_object(&((trg_column_description *)li->data)->column);

    while ((child = gtk_widget_get_first_child(GTK_WIDGET(object))))
        gtk_widget_unparent(child);
    priv->scrolled = NULL;

    g_clear_object(&priv->column_view);
    g_clear_object(&priv->list_view);
    g_clear_object(&priv->selection);
    g_clear_object(&priv->sort_model);
    g_clear_object(&priv->model);
    g_clear_object(&priv->sorter);
    g_clear_object(&priv->actions);

    G_OBJECT_CLASS(trg_tree_view_parent_class)->dispose(object);
}

static void trg_tree_view_finalize(GObject *object)
{
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(TRG_TREE_VIEW(object));

    g_list_free_full(priv->columns, (GDestroyNotify)trg_column_description_free);
    g_free(priv->configId);
    g_string_free(priv->search, TRUE);

    G_OBJECT_CLASS(trg_tree_view_parent_class)->finalize(object);
}

static void trg_tree_view_class_init(TrgTreeViewClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);
    GtkWidgetClass *widget_class = GTK_WIDGET_CLASS(klass);

    object_class->get_property = trg_tree_view_get_property;
    object_class->set_property = trg_tree_view_set_property;
    object_class->dispose = trg_tree_view_dispose;
    object_class->finalize = trg_tree_view_finalize;

    widget_class->measure = trg_tree_view_measure;
    widget_class->size_allocate = trg_tree_view_size_allocate;
    widget_class->focus = trg_tree_view_focus;
    widget_class->grab_focus = trg_tree_view_grab_focus;

    g_object_class_install_property(
        object_class, PROP_PREFS,
        g_param_spec_object("prefs", "Trg Prefs", "Trg Prefs", TRG_TYPE_PREFS,
                            G_PARAM_WRITABLE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_STRINGS));

    g_object_class_install_property(
        object_class, PROP_CONFIGID,
        g_param_spec_string("config-id", "config-id", "config-id", NULL,
                            G_PARAM_WRITABLE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_STRINGS));

    signals[SIGNAL_SORT_CHANGED]
        = g_signal_new("sort-changed", G_TYPE_FROM_CLASS(object_class), G_SIGNAL_RUN_LAST, 0, NULL,
                       NULL, NULL, G_TYPE_NONE, 0);

    signals[SIGNAL_ROW_ACTIVATED]
        = g_signal_new("row-activated", G_TYPE_FROM_CLASS(object_class), G_SIGNAL_RUN_LAST, 0, NULL,
                       NULL, NULL, G_TYPE_NONE, 1, G_TYPE_UINT);
}

static void trg_tree_view_init(TrgTreeView *tv)
{
    static const GActionEntry entries[] = {
        { "show-after", trg_tree_view_column_show_after_activate, "(ss)", NULL, NULL, { 0 } },
    };
    TrgTreeViewPrivate *priv = trg_tree_view_get_instance_private(tv);
    GtkEventController *keys;

    priv->sort_column = -1;
    priv->sort_order = GTK_SORT_ASCENDING;
    priv->sortable = TRUE;
    priv->column_menus = TRUE;
    priv->search_column = -1;
    priv->search = g_string_new(NULL);

    priv->sorter = GTK_SORTER(gtk_custom_sorter_new(trg_tree_view_sort_func, tv, NULL));
    priv->selection = GTK_SELECTION_MODEL(gtk_multi_selection_new(NULL));

    priv->column_view = g_object_ref_sink(gtk_column_view_new(g_object_ref(priv->selection)));
    gtk_column_view_set_reorderable(GTK_COLUMN_VIEW(priv->column_view), TRUE);
    gtk_column_view_set_enable_rubberband(GTK_COLUMN_VIEW(priv->column_view), TRUE);
    g_signal_connect(gtk_column_view_get_sorter(GTK_COLUMN_VIEW(priv->column_view)), "changed",
                     G_CALLBACK(trg_tree_view_column_sorter_changed), tv);
    g_signal_connect(priv->column_view, "activate", G_CALLBACK(trg_tree_view_activate_cb), tv);

    priv->scrolled = gtk_scrolled_window_new();
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(priv->scrolled), GTK_POLICY_AUTOMATIC,
                                   GTK_POLICY_AUTOMATIC);
    gtk_scrolled_window_set_has_frame(GTK_SCROLLED_WINDOW(priv->scrolled), TRUE);
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(priv->scrolled), priv->column_view);
    gtk_widget_set_parent(priv->scrolled, GTK_WIDGET(tv));

    priv->actions = g_simple_action_group_new();
    g_action_map_add_action_entries(G_ACTION_MAP(priv->actions), entries, G_N_ELEMENTS(entries),
                                    tv);
    gtk_widget_insert_action_group(GTK_WIDGET(tv), "tvcol", G_ACTION_GROUP(priv->actions));

    keys = gtk_event_controller_key_new();
    g_signal_connect(keys, "key-pressed", G_CALLBACK(trg_tree_view_key_pressed), tv);
    gtk_widget_add_controller(GTK_WIDGET(tv), keys);

    keys = gtk_event_controller_key_new();
    gtk_event_controller_set_propagation_phase(keys, GTK_PHASE_CAPTURE);
    g_signal_connect(keys, "key-pressed", G_CALLBACK(trg_tree_view_expand_key_pressed), tv);
    gtk_widget_add_controller(GTK_WIDGET(tv), keys);

    gtk_widget_set_sensitive(GTK_WIDGET(tv), FALSE);
}

GtkWidget *trg_tree_view_new(void)
{
    return GTK_WIDGET(g_object_new(TRG_TYPE_TREE_VIEW, NULL));
}
