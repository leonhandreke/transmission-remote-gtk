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
#include <math.h>
#include <stdint.h>
#if ENABLE_NL_LANGINFO
#include <langinfo.h>
#endif
#include <libsoup/soup.h>

#include "json.h"
#include "requests.h"
#include "session-get.h"
#include "trg-json-widgets.h"
#include "trg-main-window.h"
#include "trg-remote-prefs-dialog.h"
#include "util.h"

/* Using the widget creation functions in trg-json-widgets.c, load remote
 * preferences from the latest session object held by TrgClient.
 * If the user clicks OK, use trg-json-widgets to build up a request object
 * and send that in a session-set request.
 *
 * The on_session_set callback should cause that session object to be refreshed
 * as soon as the set is complete.
 */
struct _TrgRemotePrefsDialog {
    TrgDialog parent;

    TrgClient *client;
    TrgMainWindow *parent_win;

    GList *widgets;
    GtkWidget *encryption_combo;
    GtkWidget *port_test_label, *port_test_button;
    GtkWidget *blocklist_update_button, *blocklist_check;
    GtkWidget *alt_check, *alt_time_check;
    GtkWidget *alt_speed_down_spin, *alt_speed_up_spin;
    GtkWidget *alt_begin_hours_spin, *alt_begin_mins_spin;
    GtkWidget *alt_end_hours_spin, *alt_end_mins_spin;
    GtkWidget *alt_days_grid;
    GtkWidget *cache_size_label, *cache_size_spin;
    GtkWidget *blocklist_url_label, *blocklist_url_entry;
    GtkWidget *queues_label;
};

G_DEFINE_TYPE(TrgRemotePrefsDialog, trg_remote_prefs_dialog, TRG_TYPE_DIALOG)

enum {
    PROP_0,
    PROP_PARENT,
    PROP_CLIENT
};

static GObject *instance = NULL;

/* The widgets of the template that show a value of the session, which aren't
 * always there */
static const struct {
    const gchar *id;
    const gchar *key;
} session_widgets[] = {
    /* General */
    { "download_dir_entry", SGET_DOWNLOAD_DIR },
    { "incomplete_dir_check", SGET_INCOMPLETE_DIR_ENABLED },
    { "incomplete_dir_entry", SGET_INCOMPLETE_DIR },
    { "script_torrent_done_check", SGET_SCRIPT_TORRENT_DONE_ENABLED },
    { "script_torrent_done_entry", SGET_SCRIPT_TORRENT_DONE_FILENAME },
    { "rename_partial_check", SGET_RENAME_PARTIAL_FILES },
    { "trash_original_check", SGET_TRASH_ORIGINAL_TORRENT_FILES },
    { "start_added_check", SGET_START_ADDED_TORRENTS },
    /* Connections */
    { "peer_port_spin", SGET_PEER_PORT },
    { "peer_port_random_check", SGET_PEER_PORT_RANDOM_ON_START },
    { "port_forwarding_check", SGET_PORT_FORWARDING_ENABLED },
    { "pex_check", SGET_PEX_ENABLED },
    { "dht_check", SGET_DHT_ENABLED },
    { "lpd_check", SGET_LPD_ENABLED },
    { "blocklist_check", SGET_BLOCKLIST_ENABLED },
    /* Bandwidth */
    { "speed_limit_down_check", SGET_SPEED_LIMIT_DOWN_ENABLED },
    { "speed_limit_down_spin", SGET_SPEED_LIMIT_DOWN },
    { "speed_limit_up_check", SGET_SPEED_LIMIT_UP_ENABLED },
    { "speed_limit_up_spin", SGET_SPEED_LIMIT_UP },
    { "alt_check", SGET_ALT_SPEED_ENABLED },
    { "alt_time_check", SGET_ALT_SPEED_TIME_ENABLED },
    { "alt_speed_down_spin", SGET_ALT_SPEED_DOWN },
    { "alt_speed_up_spin", SGET_ALT_SPEED_UP },
    /* Limits */
    { "seed_ratio_limited_check", SGET_SEED_RATIO_LIMITED },
    { "seed_ratio_limit_spin", SGET_SEED_RATIO_LIMIT },
    { "peer_limit_global_spin", SGET_PEER_LIMIT_GLOBAL },
    { "peer_limit_per_torrent_spin", SGET_PEER_LIMIT_PER_TORRENT },
};

/* With servers that have queues */
static const struct {
    const gchar *id;
    const gchar *key;
} queue_widgets[] = {
    { "download_queue_check", SGET_DOWNLOAD_QUEUE_ENABLED },
    { "download_queue_spin", SGET_DOWNLOAD_QUEUE_SIZE },
    { "seed_queue_check", SGET_SEED_QUEUE_ENABLED },
    { "seed_queue_spin", SGET_SEED_QUEUE_SIZE },
    { "queue_stalled_check", SGET_QUEUE_STALLED_ENABLED },
    { "queue_stalled_spin", SGET_QUEUE_STALLED_MINUTES },
};

static void update_session(TrgRemotePrefsDialog *dlg)
{
    JsonNode *request = session_set();
    JsonObject *args = node_get_arguments(request);
    gchar *encryption;

    /* Connection */

    switch (gtk_drop_down_get_selected(GTK_DROP_DOWN(dlg->encryption_combo))) {
    case 0:
        encryption = "required";
        break;
    case 2:
        encryption = "tolerated";
        break;
    default:
        encryption = "preferred";
        break;
    }

    json_object_set_string_member(args, SGET_ENCRYPTION, encryption);

    trg_json_widgets_save(dlg->widgets, args);

    dispatch_rpc_async(dlg->client, request, on_session_set, dlg->parent_win);
}

static void trg_remote_prefs_response_cb(TrgRemotePrefsDialog *dlg, gint res_id,
                                         gpointer data G_GNUC_UNUSED)
{

    if (res_id == GTK_RESPONSE_OK)
        update_session(dlg);

    trg_json_widget_desc_list_free(dlg->widgets);

    gtk_window_destroy(GTK_WINDOW(dlg));
    instance = NULL;
}

static void trg_remote_prefs_dialog_get_property(GObject *object, guint property_id, GValue *value,
                                                 GParamSpec *pspec)
{
    TrgRemotePrefsDialog *self = TRG_REMOTE_PREFS_DIALOG(object);
    switch (property_id) {
    case PROP_PARENT:
        g_value_set_object(value, self->parent_win);
        break;
    case PROP_CLIENT:
        g_value_set_pointer(value, self->client);
        break;
    default:
        G_OBJECT_WARN_INVALID_PROPERTY_ID(object, property_id, pspec);
        break;
    }
}

static void trg_remote_prefs_dialog_set_property(GObject *object, guint property_id,
                                                 const GValue *value, GParamSpec *pspec)
{
    TrgRemotePrefsDialog *self = TRG_REMOTE_PREFS_DIALOG(object);
    switch (property_id) {
    case PROP_PARENT:
        self->parent_win = g_value_get_object(value);
        break;
    case PROP_CLIENT:
        self->client = g_value_get_pointer(value);
        break;
    default:
        G_OBJECT_WARN_INVALID_PROPERTY_ID(object, property_id, pspec);
        break;
    }
}

/* The alternate limits apply when they're on, or at the alternate times */
static void alt_speed_dependency_cb(GObject *object, GParamSpec *pspec, TrgRemotePrefsDialog *self)
{
    gboolean sensitive;

    if (!self->alt_speed_down_spin)
        return;

    sensitive = gtk_check_button_get_active(GTK_CHECK_BUTTON(self->alt_time_check))
        || gtk_check_button_get_active(GTK_CHECK_BUTTON(self->alt_check));

    gtk_widget_set_sensitive(self->alt_speed_down_spin, sensitive);
    gtk_widget_set_sensitive(self->alt_speed_up_spin, sensitive);
}

/* A time of day in minutes, in a spin button for the hours and one for the
 * minutes */
static void trg_rprefs_time_widget_savefunc(GtkWidget *w, JsonObject *obj, gchar *key)
{
    GtkWidget *minutesSpin = g_object_get_data(G_OBJECT(w), "mins-spin");
    gint hoursValue = gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(w));
    gint minutesValue = gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(minutesSpin));

    json_object_set_int_member(obj, key, (gint64)((hoursValue * 60) + minutesValue));
}

static void trg_rprefs_time_widget_add(TrgRemotePrefsDialog *self, JsonObject *obj,
                                       const gchar *key, GtkWidget *hourSpin,
                                       GtkWidget *minutesSpin)
{
    gint64 value = json_object_get_int_member(obj, key);
    trg_json_widget_desc *wd = g_new0(trg_json_widget_desc, 1);

    wd->key = g_strdup(key);
    wd->widget = hourSpin;
    wd->saveFunc = trg_rprefs_time_widget_savefunc;
    g_object_set_data(G_OBJECT(hourSpin), "mins-spin", minutesSpin);

    gtk_spin_button_set_value(GTK_SPIN_BUTTON(hourSpin), floor((gdouble)value / 60.0));
    gtk_spin_button_set_value(GTK_SPIN_BUTTON(minutesSpin), (gdouble)(value % 60));

    self->widgets = g_list_append(self->widgets, wd);
}

/* Two digits */
static gboolean time_spin_output(GtkSpinButton *spin, gpointer data)
{
    gchar *text = g_strdup_printf("%02d", gtk_spin_button_get_value_as_int(spin));

    gtk_editable_set_text(GTK_EDITABLE(spin), text);
    g_free(text);

    return TRUE;
}

/* The days of the week, a bit each in a number */
static void trg_rprefs_alt_days_savefunc(GtkWidget *grid, JsonObject *obj, gchar *key)
{
    guint64 days = 0;

    for (gint i = 0, x = 1; i < 7; i++, x <<= 1) {
        GtkWidget *w = gtk_grid_get_child_at(GTK_GRID(grid), i, 0);
        if (gtk_check_button_get_active(GTK_CHECK_BUTTON(w)))
            days += x;
    }

    json_object_set_int_member(obj, key, days);
}

static void trg_rprefs_alt_days_add(TrgRemotePrefsDialog *self, JsonObject *obj, const gchar *key)
{
#if ENABLE_NL_LANGINFO
    nl_item abdays[] = { ABDAY_1, ABDAY_2, ABDAY_3, ABDAY_4, ABDAY_5, ABDAY_6, ABDAY_7 };
#endif
    gint64 days = json_object_get_int_member(obj, key);
    trg_json_widget_desc *wd;

    for (gint i = 0, x = 1; i < 7; i++, x <<= 1) {
        GtkWidget *w = gtk_grid_get_child_at(GTK_GRID(self->alt_days_grid), i, 0);
#if ENABLE_NL_LANGINFO
        /* The names of the locale, rather than the translations */
        gchar *utf8 = g_convert_with_fallback(nl_langinfo(abdays[i]), -1, "utf-8",
                                              nl_langinfo(CODESET), NULL, NULL, NULL, NULL);
        if (utf8)
            gtk_check_button_set_label(GTK_CHECK_BUTTON(w), utf8);
        g_free(utf8);
#endif
        gtk_check_button_set_active(GTK_CHECK_BUTTON(w), (days & x) == x);
    }

    wd = g_new0(trg_json_widget_desc, 1);
    wd->key = g_strdup(key);
    wd->widget = self->alt_days_grid;
    wd->saveFunc = trg_rprefs_alt_days_savefunc;
    self->widgets = g_list_append(self->widgets, wd);
}

static gboolean on_port_tested(gpointer data)
{
    trg_response *response = (trg_response *)data;

    if (TRG_IS_REMOTE_PREFS_DIALOG(response->cb_data)) {
        TrgRemotePrefsDialog *self = TRG_REMOTE_PREFS_DIALOG(response->cb_data);

        gtk_button_set_label(GTK_BUTTON(self->port_test_button), _("Retest"));
        gtk_widget_set_sensitive(self->port_test_button, TRUE);

        if (response->status == SOUP_STATUS_OK) {
            gboolean isOpen
                = json_object_get_boolean_member(get_arguments(response->obj), "port-is-open");
            if (isOpen)
                gtk_label_set_markup(
                    GTK_LABEL(self->port_test_label),
                    _("Port is <span font_weight=\"bold\" fgcolor=\"darkgreen\">open</span>"));
            else
                gtk_label_set_markup(
                    GTK_LABEL(self->port_test_label),
                    _("Port is <span font_weight=\"bold\" fgcolor=\"red\">closed</span>"));
        } else {
            trg_client_error_dialog(GTK_WINDOW(response->cb_data), response);
        }
    }

    trg_response_free(response);
    return FALSE;
}

static void port_test_cb(GtkButton *b, TrgRemotePrefsDialog *self)
{
    JsonNode *req = port_test();

    gtk_label_set_text(GTK_LABEL(self->port_test_label), _("Port test"));
    gtk_button_set_label(b, _("Testing..."));
    gtk_widget_set_sensitive(GTK_WIDGET(b), FALSE);

    dispatch_rpc_async(self->client, req, on_port_tested, self);
}

static gboolean on_blocklist_updated(gpointer data)
{
    trg_response *response = (trg_response *)data;
    if (TRG_IS_REMOTE_PREFS_DIALOG(response->cb_data)) {
        TrgRemotePrefsDialog *self = TRG_REMOTE_PREFS_DIALOG(response->cb_data);

        gtk_widget_set_sensitive(self->blocklist_update_button, TRUE);
        gtk_button_set_label(GTK_BUTTON(self->blocklist_update_button), _("Update"));

        if (response->status == SOUP_STATUS_OK) {
            JsonObject *args = get_arguments(response->obj);
            gchar *labelText
                = g_strdup_printf(_("Blocklist (%" G_GINT64_FORMAT " entries)"),
                                  json_object_get_int_member(args, SGET_BLOCKLIST_SIZE));
            gtk_check_button_set_label(GTK_CHECK_BUTTON(self->blocklist_check), labelText);
            g_free(labelText);
        } else {
            trg_client_error_dialog(GTK_WINDOW(response->cb_data), response);
        }
    }

    trg_response_free(response);

    return FALSE;
}

static void update_blocklist_cb(GtkButton *b, TrgRemotePrefsDialog *self)
{
    JsonNode *req = blocklist_update();

    gtk_widget_set_sensitive(GTK_WIDGET(b), FALSE);
    gtk_button_set_label(b, _("Updating..."));

    dispatch_rpc_async(self->client, req, on_blocklist_updated, self);
}

static GtkWidget *trg_rprefs_template_child(TrgRemotePrefsDialog *self, const gchar *id)
{
    return GTK_WIDGET(
        gtk_widget_get_template_child(GTK_WIDGET(self), TRG_TYPE_REMOTE_PREFS_DIALOG, id));
}

static void trg_remote_prefs_dialog_constructed(GObject *object)
{
    TrgRemotePrefsDialog *self = TRG_REMOTE_PREFS_DIALOG(object);
    JsonObject *s = trg_client_get_session(self->client);
    const gchar *encryption = session_get_encryption(s);
    gchar *blocklistLabel;
    guint i;

    G_OBJECT_CLASS(trg_remote_prefs_dialog_parent_class)->constructed(object);

    gtk_window_set_transient_for(GTK_WINDOW(object), GTK_WINDOW(self->parent_win));

    /* These also show a value of the session */
    self->blocklist_check = trg_rprefs_template_child(self, "blocklist_check");
    self->alt_check = trg_rprefs_template_child(self, "alt_check");
    self->alt_time_check = trg_rprefs_template_child(self, "alt_time_check");
    self->alt_speed_down_spin = trg_rprefs_template_child(self, "alt_speed_down_spin");
    self->alt_speed_up_spin = trg_rprefs_template_child(self, "alt_speed_up_spin");

    for (i = 0; i < G_N_ELEMENTS(session_widgets); i++)
        trg_json_widget_add(&self->widgets, trg_rprefs_template_child(self, session_widgets[i].id),
                            s, session_widgets[i].key);

    if (json_object_has_member(s, SGET_DOWNLOAD_QUEUE_ENABLED)) {
        for (i = 0; i < G_N_ELEMENTS(queue_widgets); i++)
            trg_json_widget_add(&self->widgets,
                                trg_rprefs_template_child(self, queue_widgets[i].id), s,
                                queue_widgets[i].key);
    } else {
        gtk_widget_set_visible(self->queues_label, FALSE);
        for (i = 0; i < G_N_ELEMENTS(queue_widgets); i++)
            gtk_widget_set_visible(trg_rprefs_template_child(self, queue_widgets[i].id), FALSE);
    }

    if (session_get_cache_size_mb(s) >= 0) {
        trg_json_widget_add(&self->widgets, self->cache_size_spin, s, SGET_CACHE_SIZE_MB);
    } else {
        gtk_widget_set_visible(self->cache_size_label, FALSE);
        gtk_widget_set_visible(self->cache_size_spin, FALSE);
    }

    if (session_get_blocklist_url(s)) {
        trg_json_widget_add(&self->widgets, self->blocklist_url_entry, s, SGET_BLOCKLIST_URL);
    } else {
        gtk_widget_set_visible(self->blocklist_url_label, FALSE);
        gtk_widget_set_visible(self->blocklist_url_entry, FALSE);
    }

    trg_rprefs_time_widget_add(self, s, SGET_ALT_SPEED_TIME_BEGIN, self->alt_begin_hours_spin,
                               self->alt_begin_mins_spin);
    trg_rprefs_time_widget_add(self, s, SGET_ALT_SPEED_TIME_END, self->alt_end_hours_spin,
                               self->alt_end_mins_spin);
    trg_rprefs_alt_days_add(self, s, SGET_ALT_SPEED_TIME_DAY);
    alt_speed_dependency_cb(NULL, NULL, self);

    if (!g_strcmp0(encryption, "required"))
        gtk_drop_down_set_selected(GTK_DROP_DOWN(self->encryption_combo), 0);
    else if (!g_strcmp0(encryption, "tolerated"))
        gtk_drop_down_set_selected(GTK_DROP_DOWN(self->encryption_combo), 2);
    else
        gtk_drop_down_set_selected(GTK_DROP_DOWN(self->encryption_combo), 1);

    blocklistLabel = g_strdup_printf(_("Blocklist (%" G_GINT64_FORMAT " entries)"),
                                     session_get_blocklist_size(s));
    gtk_check_button_set_label(GTK_CHECK_BUTTON(self->blocklist_check), blocklistLabel);
    g_free(blocklistLabel);

    g_signal_connect(G_OBJECT(object), "response", G_CALLBACK(trg_remote_prefs_response_cb), NULL);
}

static void trg_remote_prefs_dialog_dispose(GObject *object)
{
    gtk_widget_dispose_template(GTK_WIDGET(object), TRG_TYPE_REMOTE_PREFS_DIALOG);

    G_OBJECT_CLASS(trg_remote_prefs_dialog_parent_class)->dispose(object);
}

static void trg_remote_prefs_dialog_class_init(TrgRemotePrefsDialogClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);
    GtkWidgetClass *widget_class = GTK_WIDGET_CLASS(klass);
    guint i;

    object_class->constructed = trg_remote_prefs_dialog_constructed;
    object_class->dispose = trg_remote_prefs_dialog_dispose;
    object_class->get_property = trg_remote_prefs_dialog_get_property;
    object_class->set_property = trg_remote_prefs_dialog_set_property;

    g_object_class_install_property(
        object_class, PROP_CLIENT,
        g_param_spec_pointer("trg-client", "TClient", "Client",
                             G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                 | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    g_object_class_install_property(
        object_class, PROP_PARENT,
        g_param_spec_object("parent-window", "Parent window", "Parent window", TRG_TYPE_MAIN_WINDOW,
                            G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    gtk_widget_class_set_template_from_resource(
        widget_class, "/io/github/TransmissionRemoteGtk/ui/trg-remote-prefs-dialog.ui");
    gtk_widget_class_bind_template_child(widget_class, TrgRemotePrefsDialog, encryption_combo);
    gtk_widget_class_bind_template_child(widget_class, TrgRemotePrefsDialog, port_test_label);
    gtk_widget_class_bind_template_child(widget_class, TrgRemotePrefsDialog, port_test_button);
    gtk_widget_class_bind_template_child(widget_class, TrgRemotePrefsDialog,
                                         blocklist_update_button);
    gtk_widget_class_bind_template_child(widget_class, TrgRemotePrefsDialog, alt_begin_hours_spin);
    gtk_widget_class_bind_template_child(widget_class, TrgRemotePrefsDialog, alt_begin_mins_spin);
    gtk_widget_class_bind_template_child(widget_class, TrgRemotePrefsDialog, alt_end_hours_spin);
    gtk_widget_class_bind_template_child(widget_class, TrgRemotePrefsDialog, alt_end_mins_spin);
    gtk_widget_class_bind_template_child(widget_class, TrgRemotePrefsDialog, alt_days_grid);
    gtk_widget_class_bind_template_child(widget_class, TrgRemotePrefsDialog, cache_size_label);
    gtk_widget_class_bind_template_child(widget_class, TrgRemotePrefsDialog, cache_size_spin);
    gtk_widget_class_bind_template_child(widget_class, TrgRemotePrefsDialog, blocklist_url_label);
    gtk_widget_class_bind_template_child(widget_class, TrgRemotePrefsDialog, blocklist_url_entry);
    gtk_widget_class_bind_template_child(widget_class, TrgRemotePrefsDialog, queues_label);

    /* Declared, for gtk_widget_get_template_child() */
    for (i = 0; i < G_N_ELEMENTS(session_widgets); i++)
        gtk_widget_class_bind_template_child_full(widget_class, session_widgets[i].id, FALSE, 0);
    for (i = 0; i < G_N_ELEMENTS(queue_widgets); i++)
        gtk_widget_class_bind_template_child_full(widget_class, queue_widgets[i].id, FALSE, 0);

    gtk_widget_class_bind_template_callback(widget_class, alt_speed_dependency_cb);
    gtk_widget_class_bind_template_callback(widget_class, time_spin_output);
    gtk_widget_class_bind_template_callback(widget_class, port_test_cb);
    gtk_widget_class_bind_template_callback(widget_class, update_blocklist_cb);
}

static void trg_remote_prefs_dialog_init(TrgRemotePrefsDialog *self)
{
    gtk_widget_init_template(GTK_WIDGET(self));
}

TrgRemotePrefsDialog *trg_remote_prefs_dialog_get_instance(TrgMainWindow *parent, TrgClient *client)
{
    if (instance == NULL) {
        instance = g_object_new(TRG_TYPE_REMOTE_PREFS_DIALOG, "parent-window", parent, "trg-client",
                                client, NULL);
    }

    return TRG_REMOTE_PREFS_DIALOG(instance);
}
