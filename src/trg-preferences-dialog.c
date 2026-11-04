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

#include "trg-item.h"
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
    GList *widgets;
    GtkWidget *notebook;
    GtkWidget *profile_del_button;
    GtkWidget *profile_combo;
    GListStore *profile_store;
    GtkWidget *full_update_check;
    GtkWidget *full_update_spin;
    GtkWidget *headers_box;
    GtkWidget *destinations_box;
};

G_DEFINE_TYPE(TrgPreferencesDialog, trg_preferences_dialog, TRG_TYPE_DIALOG)

enum {
    PROP_0,
    PROP_TRG_CLIENT,
    PROP_MAIN_WINDOW
};

static GObject *instance = NULL;

/* The widgets of the template that show a preference: entries, spin buttons
 * and check buttons. */
static const struct {
    const gchar *id;
    const gchar *key;
    int flags;
} pref_widgets[] = {
    /* Connection */
    { "profile_name_entry", TRG_PREFS_KEY_PROFILE_NAME, TRG_PREFS_PROFILE },
    { "hostname_entry", TRG_PREFS_KEY_HOSTNAME, TRG_PREFS_PROFILE },
    { "port_spin", TRG_PREFS_KEY_PORT, TRG_PREFS_PROFILE },
    { "rpc_url_path_entry", TRG_PREFS_KEY_RPC_URL_PATH, TRG_PREFS_PROFILE },
    { "username_entry", TRG_PREFS_KEY_USERNAME, TRG_PREFS_PROFILE },
    { "password_entry", TRG_PREFS_KEY_PASSWORD, TRG_PREFS_PROFILE },
    { "auto_connect_check", TRG_PREFS_KEY_AUTO_CONNECT, TRG_PREFS_PROFILE },
    { "ssl_check", TRG_PREFS_KEY_SSL, TRG_PREFS_PROFILE },
    { "ssl_validate_check", TRG_PREFS_KEY_SSL_VALIDATE, TRG_PREFS_PROFILE },
    { "timeout_spin", TRG_PREFS_KEY_TIMEOUT, TRG_PREFS_PROFILE },
    { "retries_spin", TRG_PREFS_KEY_RETRIES, TRG_PREFS_PROFILE },
    /* General */
    { "update_active_only_check", TRG_PREFS_KEY_UPDATE_ACTIVE_ONLY, TRG_PREFS_PROFILE },
    { "full_update_check", TRG_PREFS_ACTIVEONLY_FULLSYNC_ENABLED, TRG_PREFS_PROFILE },
    { "full_update_spin", TRG_PREFS_ACTIVEONLY_FULLSYNC_EVERY, TRG_PREFS_PROFILE },
    { "update_interval_spin", TRG_PREFS_KEY_UPDATE_INTERVAL, TRG_PREFS_PROFILE },
    { "session_update_interval_spin", TRG_PREFS_KEY_SESSION_UPDATE_INTERVAL, TRG_PREFS_PROFILE },
    { "start_paused_check", TRG_PREFS_KEY_START_PAUSED, TRG_PREFS_GLOBAL },
    { "add_options_dialog_check", TRG_PREFS_KEY_ADD_OPTIONS_DIALOG, TRG_PREFS_GLOBAL },
    { "delete_local_torrent_check", TRG_PREFS_KEY_DELETE_LOCAL_TORRENT, TRG_PREFS_GLOBAL },
    /* View */
    { "show_state_selector_check", TRG_PREFS_KEY_SHOW_STATE_SELECTOR, TRG_PREFS_GLOBAL },
    { "filter_dirs_check", TRG_PREFS_KEY_FILTER_DIRS, TRG_PREFS_GLOBAL },
    { "filter_trackers_check", TRG_PREFS_KEY_FILTER_TRACKERS, TRG_PREFS_GLOBAL },
    { "directories_first_check", TRG_PREFS_KEY_DIRECTORIES_FIRST, TRG_PREFS_GLOBAL },
    { "show_notebook_check", TRG_PREFS_KEY_SHOW_NOTEBOOK, TRG_PREFS_GLOBAL },
    { "system_tray_check", TRG_PREFS_KEY_SYSTEM_TRAY, TRG_PREFS_GLOBAL },
    { "minimize_to_tray_check", TRG_PREFS_KEY_MINIMIZE_TO_TRAY, TRG_PREFS_GLOBAL },
    { "add_notify_check", TRG_PREFS_KEY_ADD_NOTIFY, TRG_PREFS_GLOBAL },
    { "complete_notify_check", TRG_PREFS_KEY_COMPLETE_NOTIFY, TRG_PREFS_GLOBAL },
};

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

    if (res_id == GTK_RESPONSE_OK) {
        trg_pref_widget_save_all(TRG_PREFERENCES_DIALOG(dlg));
        trg_prefs_save(self->prefs);
    }

    trg_main_window_reload_dir_aliases(self->win);

    g_list_free_full(self->widgets, (GDestroyNotify)trg_pref_widget_desc_free);
    self->widgets = NULL;

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

    gtk_editable_set_text(GTK_EDITABLE(wd->widget), value ? value : "");
    g_free(value);
}

static void entry_save(TrgPrefs *prefs, void *wdp)
{
    trg_pref_widget_desc *wd = (trg_pref_widget_desc *)wdp;

    trg_prefs_set_string(prefs, wd->key, gtk_editable_get_text(GTK_EDITABLE(wd->widget)),
                         wd->flags);
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

static void spin_refresh(TrgPrefs *prefs, void *wdp)
{
    trg_pref_widget_desc *wd = (trg_pref_widget_desc *)wdp;
    gint value = trg_prefs_get_int(prefs, wd->key, wd->flags);

    gtk_spin_button_set_value(GTK_SPIN_BUTTON(wd->widget), value);
}

static void spin_save(TrgPrefs *prefs, void *wdp)
{
    trg_pref_widget_desc *wd = (trg_pref_widget_desc *)wdp;
    trg_prefs_set_int(prefs, wd->key, gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(wd->widget)),
                      wd->flags);
}

/* Loads the preference shown by a widget of the template, and saves it with
 * the others */
static void trg_pref_widget_add(TrgPreferencesDialog *self, GtkWidget *w, const gchar *key,
                                int flags)
{
    trg_pref_widget_desc *wd = trg_pref_widget_desc_new(w, (gchar *)key, flags);

    if (GTK_IS_CHECK_BUTTON(w)) {
        wd->saveFunc = &check_save;
        wd->refreshFunc = &check_refresh;
    } else if (GTK_IS_SPIN_BUTTON(w)) {
        wd->saveFunc = &spin_save;
        wd->refreshFunc = &spin_refresh;
    } else {
        wd->saveFunc = &entry_save;
        wd->refreshFunc = &entry_refresh;
    }

    trg_pref_widget_refresh(self, wd);
    self->widgets = g_list_append(self->widgets, wd);
}

static TrgStateSelector *trg_prefs_state_selector(TrgPreferencesDialog *self)
{
    return trg_main_window_get_state_selector(self->win);
}

static void filter_trackers_toggled_cb(GtkCheckButton *w, TrgPreferencesDialog *self)
{
    trg_state_selector_set_show_trackers(trg_prefs_state_selector(self),
                                         gtk_check_button_get_active(w));
}

static void directories_first_toggled_cb(GtkCheckButton *w, TrgPreferencesDialog *self)
{
    trg_state_selector_set_directories_first(trg_prefs_state_selector(self),
                                             gtk_check_button_get_active(w));
}

static void filter_dirs_toggled_cb(GtkCheckButton *w, TrgPreferencesDialog *self)
{
    trg_state_selector_set_show_dirs(trg_prefs_state_selector(self),
                                     gtk_check_button_get_active(w));
}

static void view_states_toggled_cb(GtkCheckButton *w, TrgPreferencesDialog *self)
{
    trg_widget_set_visible(GTK_WIDGET(trg_prefs_state_selector(self)),
                           gtk_check_button_get_active(w));
}

static void notebook_toggled_cb(GtkCheckButton *b, TrgPreferencesDialog *self)
{
    trg_main_window_notebook_set_visible(self->win, gtk_check_button_get_active(b));
}

/* The full update interval only matters when updating active torrents only
 * and doing full updates */
static void full_update_dependency_cb(GObject *object, GParamSpec *pspec,
                                      TrgPreferencesDialog *self)
{
    gtk_widget_set_sensitive(
        self->full_update_spin,
        gtk_widget_get_sensitive(self->full_update_check)
            && gtk_check_button_get_active(GTK_CHECK_BUTTON(self->full_update_check)));
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
    return gtk_drop_down_get_selected_item(GTK_DROP_DOWN(self->profile_combo));
}

static void profile_changed_cb(GtkDropDown *combo, GParamSpec *pspec, TrgPreferencesDialog *self)
{
    TrgItem *item;

    trg_pref_widget_save_all(self);

    if ((item = trg_prefs_profile_combo_get_selected(self))) {
        trg_prefs_set_profile(self->prefs, trg_item_get_pointer(item, PROFILE_COLUMN_PROFILE));
        trg_pref_widget_refresh_all(self);
        gtk_widget_set_sensitive(self->profile_del_button,
                                 g_list_model_get_n_items(G_LIST_MODEL(self->profile_store)) > 1);
    } else {
        gtk_widget_set_sensitive(self->profile_del_button, FALSE);
        gtk_drop_down_set_selected(combo, 0);
    }
}

static TrgItem *trg_prefs_profile_item_new(JsonObject *profile, const gchar *name)
{
    TrgItem *item = g_object_new(TRG_TYPE_PROFILE_ITEM, NULL);

    trg_item_set(item, PROFILE_COLUMN_PROFILE, profile, PROFILE_COLUMN_NAME, name, -1);

    return item;
}

static void trg_prefs_profile_combo_populate(TrgPreferencesDialog *self)
{
    gint profile_id = trg_prefs_get_int(self->prefs, TRG_PREFS_KEY_PROFILE_ID, TRG_PREFS_GLOBAL);
    GList *profiles = json_array_get_elements(trg_prefs_get_profiles(self->prefs));
    GList *li;

    /* Selecting the current profile isn't a change */
    g_signal_handlers_block_by_func(self->profile_combo, profile_changed_cb, self);

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
        g_list_store_append(self->profile_store, item);
        g_object_unref(item);
    }

    if (profile_id >= 0
        && (guint)profile_id < g_list_model_get_n_items(G_LIST_MODEL(self->profile_store)))
        gtk_drop_down_set_selected(GTK_DROP_DOWN(self->profile_combo), profile_id);

    g_signal_handlers_unblock_by_func(self->profile_combo, profile_changed_cb, self);

    gtk_widget_set_sensitive(self->profile_del_button, g_list_length(profiles) > 1);

    g_list_free(profiles);
}

static void name_changed_cb(GtkEditable *w, TrgPreferencesDialog *self)
{
    TrgItem *item = trg_prefs_profile_combo_get_selected(self);

    if (item)
        trg_item_set(item, PROFILE_COLUMN_NAME, gtk_editable_get_text(w), -1);
}

static void del_profile_cb(GtkButton *w, TrgPreferencesDialog *self)
{
    GtkDropDown *combo = GTK_DROP_DOWN(self->profile_combo);
    guint position = gtk_drop_down_get_selected(combo);
    TrgItem *item = trg_prefs_profile_combo_get_selected(self);

    if (item) {
        trg_prefs_del_profile(self->prefs, trg_item_get_pointer(item, PROFILE_COLUMN_PROFILE));
        trg_prefs_set_profile(self->prefs, NULL);
        g_list_store_remove(self->profile_store, position);
        gtk_drop_down_set_selected(combo, 0);
    }
}

static void add_profile_cb(GtkButton *w, TrgPreferencesDialog *self)
{
    JsonObject *profile = trg_prefs_new_profile(self->prefs);
    TrgItem *item = trg_prefs_profile_item_new(profile, _(TRG_PROFILE_NAME_DEFAULT));

    g_list_store_append(self->profile_store, item);
    g_object_unref(item);
    gtk_drop_down_set_selected(GTK_DROP_DOWN(self->profile_combo),
                               g_list_model_get_n_items(G_LIST_MODEL(self->profile_store)) - 1);
}

/* A list of label/value pairs (the remote download directories, the custom
 * headers), in a box of the template */
static void trg_prefs_add_persistent_list(TrgPreferencesDialog *self, GtkWidget *box,
                                          const gchar *key, int flags, const gchar *subkey1,
                                          const gchar *title1, const gchar *subkey2,
                                          const gchar *title2)
{
    TrgPersistentTreeView *ptv = trg_persistent_tree_view_new(key, flags);
    trg_pref_widget_desc *wd;

    trg_persistent_tree_view_set_add_select(
        ptv, trg_persistent_tree_view_add_column(ptv, 0, subkey1, title1));
    trg_persistent_tree_view_add_column(ptv, 1, subkey2, title2);
    wd = trg_persistent_tree_view_get_widget_desc(ptv);
    trg_pref_widget_refresh(self, wd);
    self->widgets = g_list_append(self->widgets, wd);

    gtk_widget_set_vexpand(GTK_WIDGET(ptv), TRUE);
    gtk_box_append(GTK_BOX(box), GTK_WIDGET(ptv));
}

static void trg_preferences_dialog_constructed(GObject *object)
{
    TrgPreferencesDialog *self = TRG_PREFERENCES_DIALOG(object);
    guint i;

    G_OBJECT_CLASS(trg_preferences_dialog_parent_class)->constructed(object);

    gtk_window_set_transient_for(GTK_WINDOW(object), GTK_WINDOW(self->win));

    trg_prefs_profile_combo_populate(self);

    for (i = 0; i < G_N_ELEMENTS(pref_widgets); i++) {
        GtkWidget *w = GTK_WIDGET(gtk_widget_get_template_child(
            GTK_WIDGET(self), TRG_TYPE_PREFERENCES_DIALOG, pref_widgets[i].id));
        trg_pref_widget_add(self, w, pref_widgets[i].key, pref_widgets[i].flags);
    }

    trg_prefs_add_persistent_list(self, self->headers_box, TRG_PREFS_KEY_CUSTOM_HEADERS,
                                  TRG_PREFS_PROFILE, TRG_PREFS_KEY_CUSTOM_HEADER_NAME, _("Name"),
                                  TRG_PREFS_KEY_CUSTOM_HEADER_VALUE, _("Value"));
    trg_prefs_add_persistent_list(self, self->destinations_box, TRG_PREFS_KEY_DESTINATIONS,
                                  TRG_PREFS_CONNECTION, TRG_PREFS_SUBKEY_LABEL, _("Label"),
                                  TRG_PREFS_KEY_DESTINATIONS_SUBKEY_DIR, _("Directory"));

    full_update_dependency_cb(NULL, NULL, self);

    g_signal_connect(G_OBJECT(object), "response", G_CALLBACK(trg_preferences_response_cb), NULL);
}

void trg_preferences_dialog_set_page(TrgPreferencesDialog *pref_dlg, guint page)
{
    gtk_notebook_set_current_page(GTK_NOTEBOOK(pref_dlg->notebook), page);
}

static void trg_preferences_dialog_init(TrgPreferencesDialog *self)
{
    g_type_ensure(TRG_TYPE_PROFILE_ITEM);
    gtk_widget_init_template(GTK_WIDGET(self));
}

static void trg_preferences_dialog_dispose(GObject *object)
{
    gtk_widget_dispose_template(GTK_WIDGET(object), TRG_TYPE_PREFERENCES_DIALOG);

    G_OBJECT_CLASS(trg_preferences_dialog_parent_class)->dispose(object);
}

static void trg_preferences_dialog_class_init(TrgPreferencesDialogClass *class)
{
    GObjectClass *g_object_class = (GObjectClass *)class;
    GtkWidgetClass *widget_class = GTK_WIDGET_CLASS(class);
    guint i;

    g_object_class->constructed = trg_preferences_dialog_constructed;
    g_object_class->dispose = trg_preferences_dialog_dispose;
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

    gtk_widget_class_set_template_from_resource(
        widget_class, "/io/github/TransmissionRemoteGtk/ui/trg-preferences-dialog.ui");
    gtk_widget_class_bind_template_child(widget_class, TrgPreferencesDialog, notebook);
    gtk_widget_class_bind_template_child(widget_class, TrgPreferencesDialog, profile_del_button);
    gtk_widget_class_bind_template_child(widget_class, TrgPreferencesDialog, profile_combo);
    gtk_widget_class_bind_template_child(widget_class, TrgPreferencesDialog, profile_store);
    gtk_widget_class_bind_template_child(widget_class, TrgPreferencesDialog, full_update_check);
    gtk_widget_class_bind_template_child(widget_class, TrgPreferencesDialog, full_update_spin);
    gtk_widget_class_bind_template_child(widget_class, TrgPreferencesDialog, headers_box);
    gtk_widget_class_bind_template_child(widget_class, TrgPreferencesDialog, destinations_box);
    /* Declared, for gtk_widget_get_template_child() */
    for (i = 0; i < G_N_ELEMENTS(pref_widgets); i++)
        gtk_widget_class_bind_template_child_full(widget_class, pref_widgets[i].id, FALSE, 0);

    gtk_widget_class_bind_template_callback(widget_class, profile_changed_cb);
    gtk_widget_class_bind_template_callback(widget_class, name_changed_cb);
    gtk_widget_class_bind_template_callback(widget_class, add_profile_cb);
    gtk_widget_class_bind_template_callback(widget_class, del_profile_cb);
    gtk_widget_class_bind_template_callback(widget_class, full_update_dependency_cb);
    gtk_widget_class_bind_template_callback(widget_class, view_states_toggled_cb);
    gtk_widget_class_bind_template_callback(widget_class, filter_dirs_toggled_cb);
    gtk_widget_class_bind_template_callback(widget_class, filter_trackers_toggled_cb);
    gtk_widget_class_bind_template_callback(widget_class, directories_first_toggled_cb);
    gtk_widget_class_bind_template_callback(widget_class, notebook_toggled_cb);
}

GtkWidget *trg_preferences_dialog_get_instance(TrgMainWindow *win, TrgClient *client)
{
    if (!instance)
        instance = g_object_new(TRG_TYPE_PREFERENCES_DIALOG, "main-window", win, "trg-client",
                                client, NULL);

    return GTK_WIDGET(instance);
}
