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

#include <glib-object.h>
#include <glib.h>
#include <glib/gi18n.h>
#include <glib/gprintf.h>
#include <gtk/gtk.h>

#include "hig.h"
#include "trg-item.h"
#include "trg-json-widgets.h"
#include "trg-main-window.h"
#include "trg-persistent-tree-view.h"
#include "trg-preferences-dialog.h"
#include "trg-prefs.h"
#include "util.h"

/* UI frontend to modify configurables stored in TrgConf.
 * To avoid lots of repetitive code, use our own functions for creating widgets
 * which also create a trg_pref_widget_desc structure and add it to a list.
 * This contains details of the config key, config flags etc, and a function
 * pointer for a save/display function. These are all called on save/load.
 */

struct _TrgPreferencesDialog {
    TrgDialog parent;

    TrgMainWindow *win;
    TrgClient *client;
    TrgPrefs *prefs;
    GtkWidget *profileDelButton;
    GtkWidget *profileComboBox;
    GtkWidget *profileNameEntry;
    GtkWidget *fullUpdateCheck;
    GList *widgets;
    GtkWidget *notebook;
};

G_DEFINE_TYPE(TrgPreferencesDialog, trg_preferences_dialog, TRG_TYPE_DIALOG)

enum {
    PROP_0,
    PROP_TRG_CLIENT,
    PROP_MAIN_WINDOW
};

static GObject *instance = NULL;

static void trg_pref_widget_desc_free(trg_pref_widget_desc *wd)
{
    g_free(wd->key);
    g_free(wd);
}

trg_pref_widget_desc *trg_pref_widget_desc_new(GtkWidget *w, gchar *key, int flags)
{
    trg_pref_widget_desc *desc = g_new0(trg_pref_widget_desc, 1);
    desc->widget = w;
    desc->key = g_strdup(key);
    desc->flags = flags;
    return desc;
}

static void trg_pref_widget_refresh(TrgPreferencesDialog *self, trg_pref_widget_desc *wd)
{
    wd->refreshFunc(self->prefs, wd);
}

static void trg_pref_widget_refresh_all(TrgPreferencesDialog *self)
{
    GList *li;
    for (li = self->widgets; li; li = g_list_next(li))
        trg_pref_widget_refresh(self, (trg_pref_widget_desc *)li->data);
}

static void trg_pref_widget_save(TrgPreferencesDialog *dlg, trg_pref_widget_desc *wd)
{
    wd->saveFunc(dlg->prefs, wd);
}

static void trg_pref_widget_save_all(TrgPreferencesDialog *dlg)
{
    GList *li;

    if (trg_prefs_get_profile(dlg->prefs) == NULL)
        return;

    trg_client_configlock(dlg->client);
    for (li = dlg->widgets; li; li = g_list_next(li)) {
        trg_pref_widget_desc *wd = (trg_pref_widget_desc *)li->data;
        trg_pref_widget_save(dlg, wd);
    }
    trg_client_configunlock(dlg->client);
}

static void trg_preferences_dialog_set_property(GObject *object, guint prop_id, const GValue *value,
                                                GParamSpec *pspec G_GNUC_UNUSED)
{
    TrgPreferencesDialog *self = TRG_PREFERENCES_DIALOG(object);

    switch (prop_id) {
    case PROP_MAIN_WINDOW:
        self->win = g_value_get_object(value);
        break;
    case PROP_TRG_CLIENT:
        self->client = g_value_get_pointer(value);
        self->prefs = trg_client_get_prefs(self->client);
        break;
    }
}

static void trg_preferences_response_cb(TrgDialog *dlg, gint res_id, gpointer data G_GNUC_UNUSED)
{
    TrgPreferencesDialog *self = TRG_PREFERENCES_DIALOG(dlg);
    GList *li;

    if (res_id == GTK_RESPONSE_OK) {
        trg_pref_widget_save_all(TRG_PREFERENCES_DIALOG(dlg));
        trg_prefs_save(self->prefs);
    }

    trg_main_window_reload_dir_aliases(self->win);

    for (li = self->widgets; li; li = g_list_next(li))
        trg_pref_widget_desc_free((trg_pref_widget_desc *)li->data);
    g_list_free(self->widgets);

    gtk_window_destroy(GTK_WINDOW(dlg));
    instance = NULL;
}

static void trg_preferences_dialog_get_property(GObject *object, guint prop_id, GValue *value,
                                                GParamSpec *pspec G_GNUC_UNUSED)
{
    TrgPreferencesDialog *self = TRG_PREFERENCES_DIALOG(object);

    switch (prop_id) {
    case PROP_MAIN_WINDOW:
        g_value_set_object(value, self->win);
        break;
    case PROP_TRG_CLIENT:
        g_value_set_pointer(value, self->client);
        break;
    }
}

static void entry_refresh(TrgPrefs *prefs, void *wdp)
{
    trg_pref_widget_desc *wd = (trg_pref_widget_desc *)wdp;
    gchar *value = trg_prefs_get_string(prefs, wd->key, wd->flags);

    if (value) {
        gtk_editable_set_text(GTK_EDITABLE(wd->widget), value);
        g_free(value);
    } else {
        gtk_editable_set_text(GTK_EDITABLE(wd->widget), "");
    }
}

static void entry_save(TrgPrefs *prefs, void *wdp)
{
    trg_pref_widget_desc *wd = (trg_pref_widget_desc *)wdp;

    trg_prefs_set_string(prefs, wd->key, gtk_editable_get_text(GTK_EDITABLE(wd->widget)),
                         wd->flags);
}

static GtkWidget *trgp_entry_new(TrgPreferencesDialog *self, gchar *key, int flags)
{
    GtkWidget *w = gtk_entry_new();
    trg_pref_widget_desc *wd = trg_pref_widget_desc_new(w, key, flags);

    wd->saveFunc = &entry_save;
    wd->refreshFunc = &entry_refresh;

    entry_refresh(self->prefs, wd);
    self->widgets = g_list_append(self->widgets, wd);

    return w;
}

static void check_refresh(TrgPrefs *prefs, void *wdp)
{
    trg_pref_widget_desc *wd = (trg_pref_widget_desc *)wdp;
    gboolean value = trg_prefs_get_bool(prefs, wd->key, wd->flags);

    gtk_check_button_set_active(GTK_CHECK_BUTTON(wd->widget), value);
}

static void check_save(TrgPrefs *prefs, void *wdp)
{
    trg_pref_widget_desc *wd = (trg_pref_widget_desc *)wdp;
    trg_prefs_set_bool(prefs, wd->key, gtk_check_button_get_active(GTK_CHECK_BUTTON(wd->widget)),
                       wd->flags);
}

static void trgp_toggle_dependent(GtkCheckButton *b, gpointer data)
{
    gtk_widget_set_sensitive(GTK_WIDGET(data), gtk_check_button_get_active(b));
}

static GtkWidget *trgp_check_new(TrgPreferencesDialog *dlg, const char *mnemonic, gchar *key,
                                 int flags, GtkCheckButton *dependency)
{
    GtkWidget *w = gtk_check_button_new_with_mnemonic(mnemonic);

    trg_pref_widget_desc *wd = trg_pref_widget_desc_new(w, key, flags);
    wd->saveFunc = &check_save;
    wd->refreshFunc = &check_refresh;
    check_refresh(dlg->prefs, wd);

    if (dependency) {
        g_signal_connect(dependency, "toggled", G_CALLBACK(trgp_toggle_dependent), w);
        gtk_widget_set_sensitive(w, gtk_check_button_get_active(dependency));
    }

    dlg->widgets = g_list_append(dlg->widgets, wd);

    return w;
}

static void spin_refresh(TrgPrefs *prefs, void *wdp)
{
    trg_pref_widget_desc *wd = (trg_pref_widget_desc *)wdp;
    GtkWidget *widget = wd->widget;

    gint value = trg_prefs_get_int(prefs, wd->key, wd->flags);
    gtk_spin_button_set_value(GTK_SPIN_BUTTON(widget), value);
}

static void spin_save(TrgPrefs *prefs, void *wdp)
{
    trg_pref_widget_desc *wd = (trg_pref_widget_desc *)wdp;
    trg_prefs_set_int(prefs, wd->key, gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(wd->widget)),
                      wd->flags);
}

static GtkWidget *trgp_spin_new(TrgPreferencesDialog *dlg, gchar *key, int low, int high, int step,
                                int flags, GtkCheckButton *dependency)
{
    GtkWidget *w = gtk_spin_button_new_with_range(low, high, step);
    trg_pref_widget_desc *wd = trg_pref_widget_desc_new(w, key, flags);

    gtk_spin_button_set_digits(GTK_SPIN_BUTTON(w), 0);

    wd->saveFunc = &spin_save;
    wd->refreshFunc = &spin_refresh;

    if (dependency) {
        g_signal_connect(dependency, "toggled", G_CALLBACK(trgp_toggle_dependent), w);
        gtk_widget_set_sensitive(w, gtk_check_button_get_active(dependency));
    }

    spin_refresh(dlg->prefs, wd);
    dlg->widgets = g_list_append(dlg->widgets, wd);

    return w;
}

static void toggle_filter_trackers(GtkCheckButton *w, gpointer win)
{
    TrgStateSelector *selector = trg_main_window_get_state_selector(TRG_MAIN_WINDOW(win));
    trg_state_selector_set_show_trackers(selector, gtk_check_button_get_active(w));
}

static void toggle_directories_first(GtkCheckButton *w, gpointer win)
{
    TrgStateSelector *selector = trg_main_window_get_state_selector(TRG_MAIN_WINDOW(win));
    trg_state_selector_set_directories_first(selector, gtk_check_button_get_active(w));
}

static void menu_bar_toggle_filter_dirs(GtkCheckButton *w, gpointer win)
{
    TrgStateSelector *selector = trg_main_window_get_state_selector(TRG_MAIN_WINDOW(win));
    trg_state_selector_set_show_dirs(selector, gtk_check_button_get_active(w));
}

static void view_states_toggled_cb(GtkCheckButton *w, gpointer data)
{
    trg_widget_set_visible(GTK_WIDGET(trg_main_window_get_state_selector(TRG_MAIN_WINDOW(data))),
                           gtk_check_button_get_active(w));
}

static void notebook_toggled_cb(GtkCheckButton *b, gpointer data)
{
    trg_main_window_notebook_set_visible(TRG_MAIN_WINDOW(data), gtk_check_button_get_active(b));
}

static void trgp_double_special_dependent(GtkWidget *widget, gpointer data)
{
    TrgPreferencesDialog *self = TRG_PREFERENCES_DIALOG(gtk_widget_get_root(widget));
    gtk_widget_set_sensitive(
        GTK_WIDGET(data),
        gtk_check_button_get_active(GTK_CHECK_BUTTON(widget))
            && gtk_widget_get_sensitive(self->fullUpdateCheck)
            && gtk_check_button_get_active(GTK_CHECK_BUTTON(self->fullUpdateCheck)));
}

static GtkWidget *trg_prefs_generalPage(TrgPreferencesDialog *dlg)
{
    GtkWidget *w, *activeOnly, *t;
    guint row = 0;

    t = hig_workarea_create();

    hig_workarea_add_section_title(t, &row, _("Updates"));

    activeOnly = w = trgp_check_new(dlg, _("Update active torrents only"),
                                    TRG_PREFS_KEY_UPDATE_ACTIVE_ONLY, TRG_PREFS_PROFILE, NULL);
    hig_workarea_add_wide_control(t, &row, w);

    dlg->fullUpdateCheck = trgp_check_new(dlg, _("Full update every (?) updates"),
                                          TRG_PREFS_ACTIVEONLY_FULLSYNC_ENABLED, TRG_PREFS_PROFILE,
                                          GTK_CHECK_BUTTON(activeOnly));
    w = trgp_spin_new(dlg, TRG_PREFS_ACTIVEONLY_FULLSYNC_EVERY, 2, INT_MAX, 1, TRG_PREFS_PROFILE,
                      GTK_CHECK_BUTTON(dlg->fullUpdateCheck));
    g_signal_connect(activeOnly, "toggled", G_CALLBACK(trgp_double_special_dependent), w);

    hig_workarea_add_row_w(t, &row, dlg->fullUpdateCheck, w, NULL);

    w = trgp_spin_new(dlg, TRG_PREFS_KEY_UPDATE_INTERVAL, 1, INT_MAX, 1, TRG_PREFS_PROFILE, NULL);
    hig_workarea_add_row(t, &row, _("Update interval:"), w, NULL);

    w = trgp_spin_new(dlg, TRG_PREFS_KEY_SESSION_UPDATE_INTERVAL, 1, INT_MAX, 1, TRG_PREFS_PROFILE,
                      NULL);
    hig_workarea_add_row(t, &row, _("Session update interval:"), w, NULL);

    hig_workarea_add_section_title(t, &row, _("Torrents"));

    w = trgp_check_new(dlg, _("Start paused"), TRG_PREFS_KEY_START_PAUSED, TRG_PREFS_GLOBAL, NULL);
    hig_workarea_add_wide_control(t, &row, w);

    w = trgp_check_new(dlg, _("Options dialog on add"), TRG_PREFS_KEY_ADD_OPTIONS_DIALOG,
                       TRG_PREFS_GLOBAL, NULL);
    hig_workarea_add_wide_control(t, &row, w);

    w = trgp_check_new(dlg, _("Delete local .torrent file after adding"),
                       TRG_PREFS_KEY_DELETE_LOCAL_TORRENT, TRG_PREFS_GLOBAL, NULL);
    hig_workarea_add_wide_control(t, &row, w);

    return t;
}

/* The items of the profile drop down */
enum {
    PROFILE_COLUMN_PROFILE,
    PROFILE_COLUMN_NAME
};

static const TrgItemColumn profile_item_columns[] = {
    { "profile", G_TYPE_POINTER },
    { "name", G_TYPE_STRING },
};

#define TRG_TYPE_PROFILE_ITEM trg_profile_item_get_type()
TRG_DEFINE_ITEM_TYPE(TrgProfileItem, trg_profile_item, TRG, PROFILE_ITEM, profile_item_columns)

static TrgItem *trg_prefs_profile_combo_get_selected(TrgPreferencesDialog *self)
{
    return gtk_drop_down_get_selected_item(GTK_DROP_DOWN(self->profileComboBox));
}

static void profile_changed_cb(GtkWidget *w, GParamSpec *pspec, gpointer data)
{
    TrgPreferencesDialog *self = TRG_PREFERENCES_DIALOG(data);
    GListModel *model = gtk_drop_down_get_model(GTK_DROP_DOWN(w));
    TrgItem *item;

    trg_pref_widget_save_all(TRG_PREFERENCES_DIALOG(data));

    if ((item = trg_prefs_profile_combo_get_selected(self))) {
        trg_prefs_set_profile(self->prefs, trg_item_get_pointer(item, PROFILE_COLUMN_PROFILE));
        trg_pref_widget_refresh_all(TRG_PREFERENCES_DIALOG(data));
        gtk_widget_set_sensitive(self->profileDelButton, g_list_model_get_n_items(model) > 1);
    } else {
        gtk_widget_set_sensitive(self->profileDelButton, FALSE);
        gtk_drop_down_set_selected(GTK_DROP_DOWN(w), 0);
    }
}

static TrgItem *trg_prefs_profile_item_new(JsonObject *profile, const gchar *name)
{
    TrgItem *item = g_object_new(TRG_TYPE_PROFILE_ITEM, NULL);

    trg_item_set(item, PROFILE_COLUMN_PROFILE, profile, PROFILE_COLUMN_NAME, name, -1);

    return item;
}

static void trg_prefs_profile_combo_populate(TrgPreferencesDialog *dialog, GtkDropDown *combo,
                                             TrgPrefs *prefs)
{
    gint profile_id = trg_prefs_get_int(prefs, TRG_PREFS_KEY_PROFILE_ID, TRG_PREFS_GLOBAL);
    GListStore *store = G_LIST_STORE(gtk_drop_down_get_model(combo));
    GList *profiles = json_array_get_elements(trg_prefs_get_profiles(prefs));
    GList *li;

    for (li = profiles; li; li = g_list_next(li)) {
        JsonObject *profile = json_node_get_object((JsonNode *)li->data);
        const gchar *name_value;
        TrgItem *item;

        if (json_object_has_member(profile, TRG_PREFS_KEY_PROFILE_NAME)) {
            name_value = json_object_get_string_member(profile, TRG_PREFS_KEY_PROFILE_NAME);
        } else {
            name_value = _(TRG_PROFILE_NAME_DEFAULT);
        }

        item = trg_prefs_profile_item_new(profile, name_value);
        g_list_store_append(store, item);
        g_object_unref(item);
    }

    if (profile_id >= 0 && (guint)profile_id < g_list_model_get_n_items(G_LIST_MODEL(store)))
        gtk_drop_down_set_selected(combo, profile_id);

    gtk_widget_set_sensitive(dialog->profileDelButton, g_list_length(profiles) > 1);

    g_list_free(profiles);
}

static void trg_prefs_profile_label_setup(GtkSignalListItemFactory *factory, GtkListItem *list_item,
                                          gpointer data)
{
    GtkWidget *label = gtk_label_new(NULL);

    gtk_label_set_xalign(GTK_LABEL(label), 0);
    gtk_list_item_set_child(list_item, label);
}

/* Bound to the name, so the label follows the name entry */
static void trg_prefs_profile_label_bind(GtkSignalListItemFactory *factory, GtkListItem *list_item,
                                         gpointer data)
{
    GBinding *binding = g_object_bind_property(gtk_list_item_get_item(list_item), "name",
                                               gtk_list_item_get_child(list_item), "label",
                                               G_BINDING_SYNC_CREATE);

    g_object_set_data(G_OBJECT(list_item), "binding", binding);
}

static void trg_prefs_profile_label_unbind(GtkSignalListItemFactory *factory,
                                           GtkListItem *list_item, gpointer data)
{
    GBinding *binding = g_object_get_data(G_OBJECT(list_item), "binding");

    if (binding)
        g_binding_unbind(binding);
    g_object_set_data(G_OBJECT(list_item), "binding", NULL);
}

static GtkWidget *trg_prefs_profile_combo_new(TrgClient *tc)
{
    GListStore *store = g_list_store_new(TRG_TYPE_PROFILE_ITEM);
    GtkListItemFactory *factory = gtk_signal_list_item_factory_new();
    GtkWidget *w = gtk_drop_down_new(G_LIST_MODEL(store), NULL);

    g_signal_connect(factory, "setup", G_CALLBACK(trg_prefs_profile_label_setup), NULL);
    g_signal_connect(factory, "bind", G_CALLBACK(trg_prefs_profile_label_bind), NULL);
    g_signal_connect(factory, "unbind", G_CALLBACK(trg_prefs_profile_label_unbind), NULL);
    gtk_drop_down_set_factory(GTK_DROP_DOWN(w), factory);
    g_object_unref(factory);

    return w;
}

static void name_changed_cb(GtkWidget *w, gpointer data)
{
    TrgPreferencesDialog *self = TRG_PREFERENCES_DIALOG(data);
    TrgItem *item = trg_prefs_profile_combo_get_selected(self);

    if (item)
        trg_item_set(item, PROFILE_COLUMN_NAME, gtk_editable_get_text(GTK_EDITABLE(w)), -1);
}

static void del_profile_cb(GtkWidget *w, gpointer data)
{
    GtkWidget *win = GTK_WIDGET(gtk_widget_get_root(w));
    TrgPreferencesDialog *self = TRG_PREFERENCES_DIALOG(win);
    TrgPrefs *prefs = self->prefs;
    GtkDropDown *combo = GTK_DROP_DOWN(data);
    guint position = gtk_drop_down_get_selected(combo);
    TrgItem *item = trg_prefs_profile_combo_get_selected(self);

    if (item) {
        trg_prefs_del_profile(prefs, trg_item_get_pointer(item, PROFILE_COLUMN_PROFILE));
        trg_prefs_set_profile(prefs, NULL);
        g_list_store_remove(G_LIST_STORE(gtk_drop_down_get_model(combo)), position);
        gtk_drop_down_set_selected(combo, 0);
    }
}

static void add_profile_cb(GtkWidget *w, gpointer data)
{
    GtkWidget *win = GTK_WIDGET(gtk_widget_get_root(w));
    TrgPreferencesDialog *self = TRG_PREFERENCES_DIALOG(win);
    GtkDropDown *combo = GTK_DROP_DOWN(data);
    GListStore *store = G_LIST_STORE(gtk_drop_down_get_model(combo));
    JsonObject *profile = trg_prefs_new_profile(self->prefs);
    TrgItem *item = trg_prefs_profile_item_new(profile, _(TRG_PROFILE_NAME_DEFAULT));

    g_list_store_append(store, item);
    g_object_unref(item);
    gtk_drop_down_set_selected(combo, g_list_model_get_n_items(G_LIST_MODEL(store)) - 1);
}

static GtkWidget *trg_prefs_dirsPage(TrgPreferencesDialog *dlg)
{
    GtkWidget *t;
    TrgPersistentTreeView *ptv;
    trg_pref_widget_desc *wd;
    guint row = 0;

    t = hig_workarea_create();

    hig_workarea_add_section_title(t, &row, _("Remote Download Directories"));

    ptv = trg_persistent_tree_view_new(TRG_PREFS_KEY_DESTINATIONS, TRG_PREFS_CONNECTION);
    trg_persistent_tree_view_set_add_select(
        ptv, trg_persistent_tree_view_add_column(ptv, 0, TRG_PREFS_SUBKEY_LABEL, _("Label")));
    trg_persistent_tree_view_add_column(ptv, 1, TRG_PREFS_KEY_DESTINATIONS_SUBKEY_DIR,
                                        _("Directory"));
    wd = trg_persistent_tree_view_get_widget_desc(ptv);
    trg_pref_widget_refresh(dlg, wd);
    dlg->widgets = g_list_append(dlg->widgets, wd);

    hig_workarea_add_wide_tall_control(t, &row, GTK_WIDGET(ptv));

    return t;
}

static GtkWidget *trg_prefs_viewPage(TrgPreferencesDialog *dlg)
{
    GtkWidget *w, *dep, *t, *tray;
    guint row = 0;

    t = hig_workarea_create();

    hig_workarea_add_section_title(t, &row, _("View"));

    dep = w = trgp_check_new(dlg, _("State selector"), TRG_PREFS_KEY_SHOW_STATE_SELECTOR,
                             TRG_PREFS_GLOBAL, NULL);
    g_signal_connect(G_OBJECT(w), "toggled", G_CALLBACK(view_states_toggled_cb), dlg->win);
    hig_workarea_add_wide_control(t, &row, w);

    w = trgp_check_new(dlg, _("Directory filters"), TRG_PREFS_KEY_FILTER_DIRS, TRG_PREFS_GLOBAL,
                       GTK_CHECK_BUTTON(dep));
    g_signal_connect(G_OBJECT(w), "toggled", G_CALLBACK(menu_bar_toggle_filter_dirs), dlg->win);
    hig_workarea_add_wide_control(t, &row, w);

    w = trgp_check_new(dlg, _("Tracker filters"), TRG_PREFS_KEY_FILTER_TRACKERS, TRG_PREFS_GLOBAL,
                       GTK_CHECK_BUTTON(dep));
    g_signal_connect(G_OBJECT(w), "toggled", G_CALLBACK(toggle_filter_trackers), dlg->win);
    hig_workarea_add_wide_control(t, &row, w);

    w = trgp_check_new(dlg, _("Directories first"), TRG_PREFS_KEY_DIRECTORIES_FIRST,
                       TRG_PREFS_GLOBAL, GTK_CHECK_BUTTON(dep));
    g_signal_connect(G_OBJECT(w), "toggled", G_CALLBACK(toggle_directories_first), dlg->win);
    hig_workarea_add_wide_control(t, &row, w);

    w = trgp_check_new(dlg, _("Torrent Details"), TRG_PREFS_KEY_SHOW_NOTEBOOK, TRG_PREFS_GLOBAL,
                       NULL);
    g_signal_connect(G_OBJECT(w), "toggled", G_CALLBACK(notebook_toggled_cb), dlg->win);
    hig_workarea_add_wide_control(t, &row, w);

    hig_workarea_add_section_title(t, &row, _("System Tray"));

    tray = trgp_check_new(dlg, _("Show in system tray"), TRG_PREFS_KEY_SYSTEM_TRAY,
                          TRG_PREFS_GLOBAL, NULL);

    /* There is no system tray support in the GTK 4 build */
    gtk_widget_set_sensitive(tray, FALSE);
    gtk_widget_set_tooltip_text(tray, _("System tray not supported."));

    hig_workarea_add_wide_control(t, &row, tray);

    w = trgp_check_new(dlg, _("Minimize to tray"), TRG_PREFS_KEY_MINIMIZE_TO_TRAY, TRG_PREFS_GLOBAL,
                       NULL);
    gtk_widget_set_sensitive(w, FALSE);
    gtk_widget_set_tooltip_text(w, _("System tray not supported."));
    hig_workarea_add_wide_control(t, &row, w);

    hig_workarea_add_section_title(t, &row, _("Notifications"));

    w = trgp_check_new(dlg, _("Torrent added notifications"), TRG_PREFS_KEY_ADD_NOTIFY,
                       TRG_PREFS_GLOBAL, NULL);
    hig_workarea_add_wide_control(t, &row, w);

    w = trgp_check_new(dlg, _("Torrent complete notifications"), TRG_PREFS_KEY_COMPLETE_NOTIFY,
                       TRG_PREFS_GLOBAL, NULL);
    hig_workarea_add_wide_control(t, &row, w);

    return t;
}

static GtkWidget *trg_prefs_serverPage(TrgPreferencesDialog *dlg)
{
    TrgPrefs *prefs = dlg->prefs;

    GtkWidget *w, *t, *frame, *frameHbox, *profileLabel;
    GtkWidget *profileButtonsHbox;
    GtkWidget *scrolled_window;
    TrgPersistentTreeView *ptv;
    trg_pref_widget_desc *wd;
    guint row = 0;

    t = hig_workarea_create();

    /* Profile */

    dlg->profileNameEntry = trgp_entry_new(dlg, TRG_PREFS_KEY_PROFILE_NAME, TRG_PREFS_PROFILE);

    dlg->profileComboBox = trg_prefs_profile_combo_new(dlg->client);
    profileLabel = gtk_label_new(_("Profile: "));

    profileButtonsHbox = trg_hbox_new(FALSE, 0);
    w = gtk_button_new_with_label(_("New"));
    g_signal_connect(w, "clicked", G_CALLBACK(add_profile_cb), dlg->profileComboBox);
    gtk_widget_set_margin_start(w, 4);
    gtk_widget_set_margin_end(w, 4);
    gtk_box_append(GTK_BOX(profileButtonsHbox), w);

    dlg->profileDelButton = gtk_button_new_with_label(_("Delete"));
    g_signal_connect(dlg->profileDelButton, "clicked", G_CALLBACK(del_profile_cb),
                     dlg->profileComboBox);
    gtk_widget_set_sensitive(dlg->profileDelButton, FALSE);
    gtk_widget_set_margin_start(dlg->profileDelButton, 4);
    gtk_widget_set_margin_end(dlg->profileDelButton, 4);
    gtk_box_append(GTK_BOX(profileButtonsHbox), dlg->profileDelButton);

    trg_prefs_profile_combo_populate(dlg, GTK_DROP_DOWN(dlg->profileComboBox), prefs);
    g_signal_connect(G_OBJECT(dlg->profileComboBox), "notify::selected",
                     G_CALLBACK(profile_changed_cb), dlg);

    /* Name */

    g_signal_connect(dlg->profileNameEntry, "changed", G_CALLBACK(name_changed_cb), dlg);

    hig_workarea_add_row(t, &row, _("Name:"), dlg->profileNameEntry, NULL);

    hig_workarea_add_wide_control(t, &row, profileButtonsHbox);

    hig_workarea_add_section_title(t, &row, _("Connection"));

    w = trgp_entry_new(dlg, TRG_PREFS_KEY_HOSTNAME, TRG_PREFS_PROFILE);
    hig_workarea_add_row(t, &row, _("Host:"), w, NULL);

    w = trgp_spin_new(dlg, TRG_PREFS_KEY_PORT, 1, 65535, 1, TRG_PREFS_PROFILE, NULL);
    hig_workarea_add_row(t, &row, _("Port:"), w, NULL);
    w = trgp_entry_new(dlg, TRG_PREFS_KEY_RPC_URL_PATH, TRG_PREFS_PROFILE);
    hig_workarea_add_row(t, &row, _("RPC URL Path:"), w, NULL);

    w = trgp_entry_new(dlg, TRG_PREFS_KEY_USERNAME, TRG_PREFS_PROFILE);
    hig_workarea_add_row(t, &row, _("Username:"), w, NULL);

    w = trgp_entry_new(dlg, TRG_PREFS_KEY_PASSWORD, TRG_PREFS_PROFILE);
    gtk_entry_set_visibility(GTK_ENTRY(w), FALSE);
    hig_workarea_add_row(t, &row, _("Password:"), w, NULL);

    w = trgp_check_new(dlg, _("Automatically connect"), TRG_PREFS_KEY_AUTO_CONNECT,
                       TRG_PREFS_PROFILE, NULL);
    hig_workarea_add_wide_control(t, &row, w);

    w = trgp_check_new(dlg, _("SSL"), TRG_PREFS_KEY_SSL, TRG_PREFS_PROFILE, NULL);
    hig_workarea_add_wide_control(t, &row, w);
    w = trgp_check_new(dlg, _("Validate SSL Certificate"), TRG_PREFS_KEY_SSL_VALIDATE,
                       TRG_PREFS_PROFILE, GTK_CHECK_BUTTON(w));
    hig_workarea_add_wide_control(t, &row, w);

    w = trgp_spin_new(dlg, TRG_PREFS_KEY_TIMEOUT, 1, 3600, 1, TRG_PREFS_PROFILE, NULL);
    hig_workarea_add_row(t, &row, _("Timeout:"), w, NULL);

    w = trgp_spin_new(dlg, TRG_PREFS_KEY_RETRIES, 0, 3600, 1, TRG_PREFS_PROFILE, NULL);
    hig_workarea_add_row(t, &row, _("Retries:"), w, NULL);

    hig_workarea_add_section_title(t, &row, _("Headers"));

    ptv = trg_persistent_tree_view_new(TRG_PREFS_KEY_CUSTOM_HEADERS, TRG_PREFS_PROFILE);
    trg_persistent_tree_view_set_add_select(
        ptv,
        trg_persistent_tree_view_add_column(ptv, 0, TRG_PREFS_KEY_CUSTOM_HEADER_NAME, _("Name")));
    trg_persistent_tree_view_add_column(ptv, 1, TRG_PREFS_KEY_CUSTOM_HEADER_VALUE, _("Value"));
    wd = trg_persistent_tree_view_get_widget_desc(ptv);
    trg_pref_widget_refresh(dlg, wd);
    dlg->widgets = g_list_append(dlg->widgets, wd);
    hig_workarea_add_wide_tall_control(t, &row, GTK_WIDGET(ptv));

    frame = gtk_frame_new(NULL);
    frameHbox = trg_hbox_new(FALSE, 2);
    gtk_widget_set_margin_start(profileLabel, 2);
    gtk_widget_set_margin_end(profileLabel, 2);
    gtk_box_append(GTK_BOX(frameHbox), profileLabel);
    gtk_widget_set_margin_start(dlg->profileComboBox, 4);
    gtk_widget_set_margin_end(dlg->profileComboBox, 4);
    gtk_box_append(GTK_BOX(frameHbox), dlg->profileComboBox);
    gtk_frame_set_label_widget(GTK_FRAME(frame), frameHbox);
    trg_widget_set_margins(frame, 5);
    gtk_frame_set_child(GTK_FRAME(frame), t);
    scrolled_window = gtk_scrolled_window_new();
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scrolled_window), GTK_POLICY_NEVER,
                                   GTK_POLICY_AUTOMATIC);
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scrolled_window), frame);

    return scrolled_window;
}

static GObject *trg_preferences_dialog_constructor(GType type, guint n_construct_properties,
                                                   GObjectConstructParam *construct_params)
{
    GObject *object;
    GtkWidget *notebook, *contentvbox;

    object = G_OBJECT_CLASS(trg_preferences_dialog_parent_class)
                 ->constructor(type, n_construct_properties, construct_params);
    TrgPreferencesDialog *self = TRG_PREFERENCES_DIALOG(object);

    contentvbox = trg_dialog_get_content_area(TRG_DIALOG(object));

    gtk_window_set_transient_for(GTK_WINDOW(object), GTK_WINDOW(self->win));
    gtk_window_set_destroy_with_parent(GTK_WINDOW(object), TRUE);
    trg_dialog_add_button(TRG_DIALOG(object), _("_Close"), GTK_RESPONSE_CLOSE);
    trg_dialog_add_button(TRG_DIALOG(object), _("_OK"), GTK_RESPONSE_OK);

    trg_dialog_set_default_response(TRG_DIALOG(object), GTK_RESPONSE_OK);

    gtk_window_set_title(GTK_WINDOW(object), _("Local Preferences"));
    trg_widget_set_margins(trg_dialog_get_content_area(TRG_DIALOG(object)), GUI_PAD);

    g_signal_connect(G_OBJECT(object), "response", G_CALLBACK(trg_preferences_response_cb), NULL);

    notebook = self->notebook = gtk_notebook_new();

    gtk_notebook_append_page(GTK_NOTEBOOK(notebook),
                             trg_prefs_serverPage(TRG_PREFERENCES_DIALOG(object)),
                             gtk_label_new(_("Connection")));

    gtk_notebook_append_page(GTK_NOTEBOOK(notebook),
                             trg_prefs_generalPage(TRG_PREFERENCES_DIALOG(object)),
                             gtk_label_new(_("General")));

    gtk_notebook_append_page(GTK_NOTEBOOK(notebook),
                             trg_prefs_viewPage(TRG_PREFERENCES_DIALOG(object)),
                             gtk_label_new(_("View")));

    gtk_notebook_append_page(GTK_NOTEBOOK(notebook),
                             trg_prefs_dirsPage(TRG_PREFERENCES_DIALOG(object)),
                             gtk_label_new(_("Directories")));

    trg_widget_set_margins(notebook, GUI_PAD);

    gtk_widget_set_vexpand(notebook, TRUE);
    gtk_box_append(GTK_BOX(contentvbox), notebook);

    return object;
}

void trg_preferences_dialog_set_page(TrgPreferencesDialog *pref_dlg, guint page)
{
    gtk_notebook_set_current_page(GTK_NOTEBOOK(pref_dlg->notebook), page);
}

static void trg_preferences_dialog_init(TrgPreferencesDialog *pref_dlg)
{
}

static void trg_preferences_dialog_class_init(TrgPreferencesDialogClass *class)
{
    GObjectClass *g_object_class = (GObjectClass *)class;

    g_object_class->constructor = trg_preferences_dialog_constructor;
    g_object_class->set_property = trg_preferences_dialog_set_property;
    g_object_class->get_property = trg_preferences_dialog_get_property;

    g_object_class_install_property(
        g_object_class, PROP_TRG_CLIENT,
        g_param_spec_pointer("trg-client", "TClient", "Client",
                             G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                 | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    g_object_class_install_property(
        g_object_class, PROP_MAIN_WINDOW,
        g_param_spec_object("main-window", "Main Window", "Main Window", TRG_TYPE_MAIN_WINDOW,
                            G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));
}

GtkWidget *trg_preferences_dialog_get_instance(TrgMainWindow *win, TrgClient *client)
{
    if (!instance)
        instance = g_object_new(TRG_TYPE_PREFERENCES_DIALOG, "main-window", win, "trg-client",
                                client, NULL);

    return GTK_WIDGET(instance);
}
