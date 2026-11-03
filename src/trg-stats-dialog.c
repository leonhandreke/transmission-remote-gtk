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
#include <libsoup/soup.h>

#include "hig.h"
#include "json.h"
#include "requests.h"
#include "trg-client.h"
#include "trg-main-window.h"
#include "trg-stats-dialog.h"
#include "util.h"

enum {
    PROP_0,
    PROP_PARENT,
    PROP_CLIENT
};

#define STATS_UPDATE_INTERVAL 5
/* A request that got no reply within this many intervals is given up on
 * (a reply is dropped when the client disconnects in between). */
#define STATS_STALE_TICKS 6

typedef enum {
    STAT_VERSION,
    STAT_DOWN,
    STAT_UP,
    STAT_RATIO,
    STAT_FILES_ADDED,
    STAT_SESSION_COUNT,
    STAT_ACTIVE,
    STAT_COUNT
} TrgStat;

struct _TrgStatsDialog {
    TrgDialog parent;

    TrgClient *client;
    TrgMainWindow *parent_win;
    guint update_stats_timer_tag;
    guint generation;
    guint in_flight_ticks; /* 0: no request pending */
    gboolean error_shown;
    GtkWidget *session_labels[STAT_COUNT];
    GtkWidget *cumulat_labels[STAT_COUNT];
};

G_DEFINE_TYPE(TrgStatsDialog, trg_stats_dialog, TRG_TYPE_DIALOG)

static TrgStatsDialog *instance = NULL;
/* Identifies the dialog a reply is for, so a reply that arrives after the
 * dialog was closed (or for an earlier one) is ignored. */
static guint instance_generation = 0;

static void trg_stats_dialog_get_property(GObject *object, guint property_id, GValue *value,
                                          GParamSpec *pspec)
{
    G_OBJECT_WARN_INVALID_PROPERTY_ID(object, property_id, pspec);
}

static void trg_stats_dialog_set_property(GObject *object, guint property_id, const GValue *value,
                                          GParamSpec *pspec)
{
    TrgStatsDialog *self = TRG_STATS_DIALOG(object);
    switch (property_id) {
    case PROP_CLIENT:
        self->client = g_value_get_pointer(value);
        break;
    case PROP_PARENT:
        self->parent_win = g_value_get_object(value);
        break;
    default:
        G_OBJECT_WARN_INVALID_PROPERTY_ID(object, property_id, pspec);
        break;
    }
}

static void trg_stats_dialog_dispose(GObject *object)
{
    TrgStatsDialog *self = TRG_STATS_DIALOG(object);

    /* Also when destroyed with the main window, without a response. */
    g_clear_handle_id(&self->update_stats_timer_tag, g_source_remove);
    if (instance == self)
        instance = NULL;

    G_OBJECT_CLASS(trg_stats_dialog_parent_class)->dispose(object);
}

static void trg_stats_response_cb(TrgDialog *dlg, gint res_id, gpointer data G_GNUC_UNUSED)
{
    gtk_window_destroy(GTK_WINDOW(dlg));
}

static void update_statistic(TrgStatsDialog *dlg, TrgStat stat, const gchar *session,
                             const gchar *cumulat)
{
    gtk_label_set_text(GTK_LABEL(dlg->session_labels[stat]), session);
    if (dlg->cumulat_labels[stat])
        gtk_label_set_text(GTK_LABEL(dlg->cumulat_labels[stat]), cumulat);
}

static JsonObject *get_session_arg(JsonObject *args)
{
    return json_object_get_object_member(args, "current-stats");
}

static JsonObject *get_cumulat_arg(JsonObject *args)
{
    return json_object_get_object_member(args, "cumulative-stats");
}

static void update_int_stat(TrgStatsDialog *dlg, JsonObject *args, TrgStat stat,
                            const gchar *jsonKey)
{
    gchar session_val[32];
    gchar cumulat_val[32];

    g_snprintf(session_val, sizeof(session_val), "%" G_GINT64_FORMAT,
               json_object_get_int_member(get_session_arg(args), jsonKey));
    g_snprintf(cumulat_val, sizeof(cumulat_val), "%" G_GINT64_FORMAT,
               json_object_get_int_member(get_cumulat_arg(args), jsonKey));

    update_statistic(dlg, stat, session_val, cumulat_val);
}

static void update_size_stat(TrgStatsDialog *dlg, JsonObject *args, TrgStat stat,
                             const gchar *jsonKey)
{
    gchar session_val[32];
    gchar cumulat_val[32];

    trg_strlsize(cumulat_val, json_object_get_int_member(get_cumulat_arg(args), jsonKey));
    trg_strlsize(session_val, json_object_get_int_member(get_session_arg(args), jsonKey));

    update_statistic(dlg, stat, session_val, cumulat_val);
}

static void update_ratio_stat(TrgStatsDialog *dlg, JsonObject *args, TrgStat stat,
                              const gchar *jsonKeyUp, const gchar *jsonKeyDown)
{
    gchar session_val[32];
    gchar cumulat_val[32];

    JsonObject *session = get_session_arg(args);
    const double session_up = json_object_get_double_member(session, jsonKeyUp);
    const double session_down = json_object_get_double_member(session, jsonKeyDown);

    JsonObject *cumulat = get_cumulat_arg(args);
    const double cumulat_up = json_object_get_double_member(cumulat, jsonKeyUp);
    const double cumulat_down = json_object_get_double_member(cumulat, jsonKeyDown);

    // Note: NAN and INFINITY are handled by trg_strlratio
    trg_strlratio(session_val, session_up / session_down);
    trg_strlratio(cumulat_val, cumulat_up / cumulat_down);

    update_statistic(dlg, stat, session_val, cumulat_val);
}

static void update_time_stat(TrgStatsDialog *dlg, JsonObject *args, TrgStat stat,
                             const gchar *jsonKey)
{
    gchar session_val[32];
    gchar cumulat_val[32];

    tr_strltime_long(session_val, json_object_get_int_member(get_session_arg(args), jsonKey),
                     sizeof(session_val));
    tr_strltime_long(cumulat_val, json_object_get_int_member(get_cumulat_arg(args), jsonKey),
                     sizeof(cumulat_val));

    update_statistic(dlg, stat, session_val, cumulat_val);
}

static gboolean on_stats_reply(gpointer data)
{
    trg_response *response = (trg_response *)data;
    TrgStatsDialog *dlg = instance;

    if (!dlg || dlg->generation != GPOINTER_TO_UINT(response->cb_data)) {
        trg_response_free(response);
        return FALSE;
    }

    dlg->in_flight_ticks = 0;

    if (response->status == SOUP_STATUS_OK) {
        JsonObject *args = get_arguments(response->obj);
        char versionStr[32];

        g_snprintf(versionStr, sizeof(versionStr), "Transmission %s",
                   trg_client_get_version_string(dlg->client));
        update_statistic(dlg, STAT_VERSION, versionStr, NULL);

        update_size_stat(dlg, args, STAT_UP, "uploadedBytes");
        update_size_stat(dlg, args, STAT_DOWN, "downloadedBytes");
        update_ratio_stat(dlg, args, STAT_RATIO, "uploadedBytes", "downloadedBytes");
        update_int_stat(dlg, args, STAT_FILES_ADDED, "filesAdded");
        update_int_stat(dlg, args, STAT_SESSION_COUNT, "sessionCount");
        update_time_stat(dlg, args, STAT_ACTIVE, "secondsActive");

        dlg->error_shown = FALSE;
    } else if (response->status != FAIL_HTTP_UNSUCCESSFUL && !dlg->error_shown
               && trg_client_is_connected(dlg->client)) {
        /* The main window reports (and retries) failed connections. Other
         * errors once until the next successful update, not on every
         * refresh. */
        dlg->error_shown = TRUE;
        trg_client_error_dialog(GTK_WINDOW(dlg), response);
    }

    trg_response_free(response);
    return FALSE;
}

static void trg_stats_dialog_request(TrgStatsDialog *dlg)
{
    dlg->in_flight_ticks = 1;
    dispatch_rpc_async(dlg->client, session_stats(), on_stats_reply,
                       GUINT_TO_POINTER(dlg->generation));
}

/* Runs as long as the dialog is open, so the refresh resumes after a
 * reconnect. */
static gboolean trg_update_stats_timerfunc(gpointer data)
{
    TrgStatsDialog *dlg = TRG_STATS_DIALOG(data);
    gboolean connected = trg_client_is_connected(dlg->client);

    gtk_widget_set_sensitive(trg_dialog_get_content_area(TRG_DIALOG(dlg)), connected);

    if (!connected)
        dlg->in_flight_ticks = 0;
    else if (dlg->in_flight_ticks == 0 || dlg->in_flight_ticks >= STATS_STALE_TICKS)
        trg_stats_dialog_request(dlg);
    else
        dlg->in_flight_ticks++;

    return G_SOURCE_CONTINUE;
}

static GtkWidget *trg_stats_label_new(const gchar *text, const gchar *css_class)
{
    GtkWidget *label = gtk_label_new(text);

    gtk_label_set_xalign(GTK_LABEL(label), 0.0f);
    gtk_label_set_ellipsize(GTK_LABEL(label), PANGO_ELLIPSIZE_END);
    gtk_widget_set_hexpand(label, TRUE);
    if (css_class)
        gtk_widget_add_css_class(label, css_class);

    return label;
}

static void trg_stats_add_statistic(TrgStatsDialog *dlg, GtkGrid *grid, TrgStat stat,
                                    const gchar *name, GtkWidget *session_header,
                                    GtkWidget *cumulat_header)
{
    GtkWidget *name_label = trg_stats_label_new(name, "dim-label");
    GtkWidget *session = dlg->session_labels[stat] = trg_stats_label_new(NULL, NULL);
    gint row = stat + 1;

    gtk_grid_attach(grid, name_label, 0, row, 1, 1);

    /* The version has no cumulative value, it may use both columns. */
    if (stat == STAT_VERSION) {
        gtk_grid_attach(grid, session, 1, row, 2, 1);
        gtk_accessible_update_relation(GTK_ACCESSIBLE(session), GTK_ACCESSIBLE_RELATION_LABELLED_BY,
                                       name_label, NULL, -1);
    } else {
        GtkWidget *cumulat = dlg->cumulat_labels[stat] = trg_stats_label_new(NULL, NULL);

        gtk_grid_attach(grid, session, 1, row, 1, 1);
        gtk_grid_attach(grid, cumulat, 2, row, 1, 1);
        gtk_accessible_update_relation(GTK_ACCESSIBLE(session), GTK_ACCESSIBLE_RELATION_LABELLED_BY,
                                       name_label, session_header, NULL, -1);
        gtk_accessible_update_relation(GTK_ACCESSIBLE(cumulat), GTK_ACCESSIBLE_RELATION_LABELLED_BY,
                                       name_label, cumulat_header, NULL, -1);
    }
}

static GObject *trg_stats_dialog_constructor(GType type, guint n_construct_properties,
                                             GObjectConstructParam *construct_params)
{
    GObject *obj = G_OBJECT_CLASS(trg_stats_dialog_parent_class)
                       ->constructor(type, n_construct_properties, construct_params);
    TrgStatsDialog *dlg = TRG_STATS_DIALOG(obj);
    GtkWidget *grid, *session_header, *cumulat_header;

    gtk_window_set_title(GTK_WINDOW(obj), _("Statistics"));
    gtk_window_set_transient_for(GTK_WINDOW(obj), GTK_WINDOW(dlg->parent_win));
    gtk_window_set_destroy_with_parent(GTK_WINDOW(obj), TRUE);
    trg_dialog_add_button(TRG_DIALOG(obj), _("_Close"), GTK_RESPONSE_CLOSE);

    trg_widget_set_margins(trg_dialog_get_content_area(TRG_DIALOG(obj)), GUI_PAD);

    trg_dialog_set_default_response(TRG_DIALOG(obj), GTK_RESPONSE_CLOSE);

    g_signal_connect(G_OBJECT(obj), "response", G_CALLBACK(trg_stats_response_cb), NULL);

    /* A fixed table of values: a grid of labels, three equal columns
     * (the old tree view had three 200 pixel columns). */
    grid = gtk_grid_new();
    gtk_grid_set_column_homogeneous(GTK_GRID(grid), TRUE);
    gtk_grid_set_column_spacing(GTK_GRID(grid), GUI_PAD_BIG);
    gtk_grid_set_row_spacing(GTK_GRID(grid), GUI_PAD);
    gtk_widget_set_size_request(grid, 3 * 200, -1);
    trg_widget_set_margins(grid, GUI_PAD);
    gtk_widget_set_vexpand(grid, TRUE);

    gtk_grid_attach(GTK_GRID(grid), trg_stats_label_new(_("Statistic"), "heading"), 0, 0, 1, 1);
    session_header = trg_stats_label_new(_("Session"), "heading");
    gtk_grid_attach(GTK_GRID(grid), session_header, 1, 0, 1, 1);
    cumulat_header = trg_stats_label_new(_("Cumulative"), "heading");
    gtk_grid_attach(GTK_GRID(grid), cumulat_header, 2, 0, 1, 1);

    trg_stats_add_statistic(dlg, GTK_GRID(grid), STAT_VERSION, _("Version"), session_header,
                            cumulat_header);
    trg_stats_add_statistic(dlg, GTK_GRID(grid), STAT_DOWN, _("Download Total"), session_header,
                            cumulat_header);
    trg_stats_add_statistic(dlg, GTK_GRID(grid), STAT_UP, _("Upload Total"), session_header,
                            cumulat_header);
    trg_stats_add_statistic(dlg, GTK_GRID(grid), STAT_RATIO, _("Ratio"), session_header,
                            cumulat_header);
    trg_stats_add_statistic(dlg, GTK_GRID(grid), STAT_FILES_ADDED, _("Files Added"), session_header,
                            cumulat_header);
    trg_stats_add_statistic(dlg, GTK_GRID(grid), STAT_SESSION_COUNT, _("Session Count"),
                            session_header, cumulat_header);
    trg_stats_add_statistic(dlg, GTK_GRID(grid), STAT_ACTIVE, _("Time Active"), session_header,
                            cumulat_header);

    gtk_box_append(GTK_BOX(trg_dialog_get_content_area(TRG_DIALOG(obj))), grid);

    dlg->generation = ++instance_generation;
    trg_stats_dialog_request(dlg);
    dlg->update_stats_timer_tag
        = g_timeout_add_seconds(STATS_UPDATE_INTERVAL, trg_update_stats_timerfunc, dlg);

    return obj;
}

static void trg_stats_dialog_class_init(TrgStatsDialogClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);

    object_class->get_property = trg_stats_dialog_get_property;
    object_class->set_property = trg_stats_dialog_set_property;
    object_class->constructor = trg_stats_dialog_constructor;
    object_class->dispose = trg_stats_dialog_dispose;

    g_object_class_install_property(
        object_class, PROP_PARENT,
        g_param_spec_object("parent-window", "Parent window", "Parent window", TRG_TYPE_MAIN_WINDOW,
                            G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    g_object_class_install_property(
        object_class, PROP_CLIENT,
        g_param_spec_pointer("trg-client", "TClient", "Client",
                             G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                 | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));
}

static void trg_stats_dialog_init(TrgStatsDialog *self)
{
}

TrgStatsDialog *trg_stats_dialog_get_instance(TrgMainWindow *parent, TrgClient *client)
{
    if (instance == NULL) {
        instance = g_object_new(TRG_TYPE_STATS_DIALOG, "trg-client", client, "parent-window",
                                parent, NULL);
    }

    return instance;
}
