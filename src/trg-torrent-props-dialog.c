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

#include "json.h"
#include "protocol-constants.h"
#include "requests.h"
#include "torrent.h"
#include "trg-client.h"
#include "trg-files-model.h"
#include "trg-files-tree-view.h"
#include "trg-json-widgets.h"
#include "trg-main-window.h"
#include "trg-peers-model.h"
#include "trg-peers-tree-view.h"
#include "trg-torrent-model.h"
#include "trg-torrent-props-dialog.h"
#include "trg-torrent-tree-view.h"
#include "trg-trackers-model.h"
#include "trg-trackers-tree-view.h"
#include "util.h"

/* Pretty similar to remote preferences, also using the widget creation functions
 * in trg-json-widgets.c. The torrent tree view is passed into here, which this
 * gets the selection from. If there are multiple selections, use the first to
 * populate the fields.
 *
 * Build the JSON array of torrent IDs when the dialog is created, in case the
 * selection changes before clicking OK.
 *
 * When the user clicks OK, use trg-json-widgets to populate an object with the
 * values and then send it with the IDs.
 */
struct _TrgTorrentPropsDialog {
    TrgDialog parent;

    TrgTorrentTreeView *tv;
    TrgTorrentModel *torrentModel;
    TrgClient *client;
    TrgMainWindow *parent_win;
    JsonArray *targetIds;

    GList *widgets;

    GtkWidget *notebook;
    GtkWidget *info_page;
    GtkWidget *bandwidth_priority_combo, *seed_ratio_mode_combo, *seed_ratio_limit_spin;
    GtkWidget *queue_position_label, *queue_position_spin;

    TrgPeersTreeView *peersTv;
    TrgPeersModel *peersModel;
    TrgTrackersTreeView *trackersTv;
    TrgTrackersModel *trackersModel;
    TrgFilesTreeView *filesTv;
    TrgFilesModel *filesModel;
    JsonObject *lastJson;

    GtkWidget *size_lb;
    GtkWidget *have_lb;
    GtkWidget *dl_lb;
    GtkWidget *ul_lb;
    GtkWidget *state_lb;
    GtkWidget *date_started_lb;
    GtkWidget *eta_lb;
    GtkWidget *last_activity_lb;
    GtkWidget *error_lb;
    GtkWidget *destination_lb;
    GtkWidget *hash_lb;
    GtkWidget *privacy_lb;
    GtkWidget *origin_lb;
    GtkTextBuffer *comment_buffer;
    gboolean show_details;
};

G_DEFINE_TYPE(TrgTorrentPropsDialog, trg_torrent_props_dialog, TRG_TYPE_DIALOG)

enum {
    PROP_0,
    PROP_TREEVIEW,
    PROP_TORRENT_MODEL,
    PROP_PARENT_WINDOW,
    PROP_CLIENT
};
/* The widgets of the limits page that show a field of the torrent */
static const struct {
    const gchar *id;
    const gchar *key;
} field_widgets[] = {
    { "honors_session_limits_check", FIELD_HONORS_SESSION_LIMITS },
    { "download_limited_check", FIELD_DOWNLOAD_LIMITED },
    { "download_limit_spin", FIELD_DOWNLOAD_LIMIT },
    { "upload_limited_check", FIELD_UPLOAD_LIMITED },
    { "upload_limit_spin", FIELD_UPLOAD_LIMIT },
    { "seed_ratio_limit_spin", FIELD_SEED_RATIO_LIMIT },
    { "peer_limit_spin", FIELD_PEER_LIMIT },
};

static void trg_torrent_props_dialog_set_property(GObject *object, guint prop_id,
                                                  const GValue *value,
                                                  GParamSpec *pspec G_GNUC_UNUSED)
{
    TrgTorrentPropsDialog *self = TRG_TORRENT_PROPS_DIALOG(object);
    switch (prop_id) {
    case PROP_PARENT_WINDOW:
        self->parent_win = g_value_get_object(value);
        break;
    case PROP_TREEVIEW:
        self->tv = g_value_get_object(value);
        break;
    case PROP_TORRENT_MODEL:
        self->torrentModel = g_value_get_object(value);
        break;
    case PROP_CLIENT:
        self->client = g_value_get_pointer(value);
        break;
    }
}

static void trg_torrent_props_dialog_get_property(GObject *object, guint prop_id, GValue *value,
                                                  GParamSpec *pspec G_GNUC_UNUSED)
{
    G_OBJECT_WARN_INVALID_PROPERTY_ID(object, prop_id, pspec);
}

static void trg_torrent_props_response_cb(TrgDialog *dialog, gint res_id,
                                          gpointer data G_GNUC_UNUSED)
{
    TrgTorrentPropsDialog *self = TRG_TORRENT_PROPS_DIALOG(dialog);

    if (self->show_details) {
        TrgPrefs *prefs = trg_client_get_prefs(self->client);
        gint width, height;
        gtk_window_get_default_size(GTK_WINDOW(dialog), &width, &height);
        trg_prefs_set_int(prefs, "dialog-width", width, TRG_PREFS_GLOBAL);
        trg_prefs_set_int(prefs, "dialog-height", height, TRG_PREFS_GLOBAL);

        trg_tree_view_persist(TRG_TREE_VIEW(self->peersTv),
                              TRG_TREE_VIEW_PERSIST_SORT | TRG_TREE_VIEW_PERSIST_LAYOUT);
        trg_tree_view_persist(TRG_TREE_VIEW(self->filesTv),
                              TRG_TREE_VIEW_PERSIST_SORT | TRG_TREE_VIEW_PERSIST_LAYOUT);
        trg_tree_view_persist(TRG_TREE_VIEW(self->trackersTv),
                              TRG_TREE_VIEW_PERSIST_SORT | TRG_TREE_VIEW_PERSIST_LAYOUT);
    }

    if (res_id != GTK_RESPONSE_OK) {
        json_array_unref(self->targetIds);
    } else {
        JsonNode *request = torrent_set(self->targetIds);
        JsonObject *args = node_get_arguments(request);

        json_object_set_int_member(
            args, FIELD_SEED_RATIO_MODE,
            gtk_drop_down_get_selected(GTK_DROP_DOWN(self->seed_ratio_mode_combo)));
        json_object_set_int_member(
            args, FIELD_BANDWIDTH_PRIORITY,
            (gint)gtk_drop_down_get_selected(GTK_DROP_DOWN(self->bandwidth_priority_combo)) - 1);

        trg_json_widgets_save(self->widgets, args);

        dispatch_rpc_async(self->client, request, on_generic_interactive_action_response,
                           self->parent_win);
    }

    gtk_window_destroy(GTK_WINDOW(dialog));
}

/* The ratio limit is for "Stop seeding at ratio" */
static void seed_ratio_mode_changed_cb(GtkDropDown *w, GParamSpec *pspec,
                                       TrgTorrentPropsDialog *self)
{
    if (self->seed_ratio_limit_spin)
        gtk_widget_set_sensitive(self->seed_ratio_limit_spin, gtk_drop_down_get_selected(w) == 1);
}

static void info_page_update(TrgTorrentPropsDialog *dialog, JsonObject *t, TrgTorrentItem *item)
{
    gint64 sizeWhenDone, haveValid, downloaded, uploaded, eta, activityDate, error;
    gchar *statusString;
    guint flags;
    const gchar *str;

    char buf[512];

    trg_item_get(item, TORRENT_COLUMN_SIZEWHENDONE, &sizeWhenDone, TORRENT_COLUMN_UPLOADED,
                 &uploaded, TORRENT_COLUMN_DOWNLOADED, &downloaded, TORRENT_COLUMN_HAVE_VALID,
                 &haveValid, TORRENT_COLUMN_ETA, &eta, TORRENT_COLUMN_LASTACTIVE, &activityDate,
                 TORRENT_COLUMN_STATUS, &statusString, TORRENT_COLUMN_FLAGS, &flags,
                 TORRENT_COLUMN_ERROR, &error, -1);

    if (torrent_get_is_private(t))
        str = _("Private to this tracker -- DHT and PEX disabled");
    else
        str = _("Public torrent");

    gtk_label_set_text(GTK_LABEL(dialog->privacy_lb), str);

    {
        const gchar *creator = torrent_get_creator(t);
        gint64 dateCreated = torrent_get_date_created(t);
        gchar *dateStr = epoch_to_string(dateCreated);

        if (creator && strlen(creator) > 0 && dateCreated > 0)
            g_snprintf(buf, sizeof(buf), _("Created by %1$s on %2$s"), creator, dateStr);
        else if (dateCreated > 0)
            g_snprintf(buf, sizeof(buf), _("Created on %1$s"), dateStr);
        else if (creator && strlen(creator) > 0)
            g_snprintf(buf, sizeof(buf), _("Created by %1$s"), creator);
        else
            g_strlcpy(buf, _("N/A"), sizeof(buf));

        g_free(dateStr);
        gtk_label_set_text(GTK_LABEL(dialog->origin_lb), buf);
    }

    gtk_text_buffer_set_text(dialog->comment_buffer, torrent_get_comment(t), -1);
    gtk_label_set_text(GTK_LABEL(dialog->destination_lb), torrent_get_download_dir(t));

    gtk_label_set_text(GTK_LABEL(dialog->state_lb), statusString);
    g_free(statusString);

    {
        gchar *addedStr = epoch_to_string(torrent_get_added_date(t));
        gtk_label_set_text(GTK_LABEL(dialog->date_started_lb), addedStr);
        g_free(addedStr);
    }

    /* eta */

    if (eta > 0) {
        tr_strltime_long(buf, eta, sizeof(buf));
        gtk_label_set_text(GTK_LABEL(dialog->eta_lb), buf);
    } else {
        gtk_label_set_text(GTK_LABEL(dialog->eta_lb), "");
    }

    gtk_label_set_text(GTK_LABEL(dialog->hash_lb), torrent_get_hash(t));
    gtk_label_set_text(GTK_LABEL(dialog->error_lb),
                       error ? torrent_get_errorstr(t) : _("No errors"));

    if (flags & TORRENT_FLAG_ACTIVE) {
        gtk_label_set_text(GTK_LABEL(dialog->last_activity_lb), _("Active now"));
    } else {
        gchar *activityStr = epoch_to_string(activityDate);
        gtk_label_set_text(GTK_LABEL(dialog->last_activity_lb), activityStr);
        g_free(activityStr);
    }

    tr_strlsize(buf, sizeWhenDone, sizeof(buf));
    gtk_label_set_text(GTK_LABEL(dialog->size_lb), buf);

    tr_strlsize(buf, downloaded, sizeof(buf));
    gtk_label_set_text(GTK_LABEL(dialog->dl_lb), buf);

    tr_strlsize(buf, uploaded, sizeof(buf));
    gtk_label_set_text(GTK_LABEL(dialog->ul_lb), buf);

    tr_strlsize(buf, haveValid, sizeof(buf));
    gtk_label_set_text(GTK_LABEL(dialog->have_lb), buf);
}

static void trg_props_limits_page_load(TrgTorrentPropsDialog *self, JsonObject *json)
{
    guint i;

    /* Also a field of the torrent */
    self->seed_ratio_limit_spin = GTK_WIDGET(gtk_widget_get_template_child(
        GTK_WIDGET(self), TRG_TYPE_TORRENT_PROPS_DIALOG, "seed_ratio_limit_spin"));

    for (i = 0; i < G_N_ELEMENTS(field_widgets); i++)
        trg_json_widget_add(&self->widgets,
                            GTK_WIDGET(gtk_widget_get_template_child(GTK_WIDGET(self),
                                                                     TRG_TYPE_TORRENT_PROPS_DIALOG,
                                                                     field_widgets[i].id)),
                            json, field_widgets[i].key);

    if (json_object_has_member(json, FIELD_QUEUE_POSITION)) {
        trg_json_widget_add(&self->widgets, self->queue_position_spin, json, FIELD_QUEUE_POSITION);
    } else {
        gtk_widget_set_visible(self->queue_position_label, FALSE);
        gtk_widget_set_visible(self->queue_position_spin, FALSE);
    }

    gtk_drop_down_set_selected(GTK_DROP_DOWN(self->bandwidth_priority_combo),
                               torrent_get_bandwidth_priority(json) + 1);
    gtk_drop_down_set_selected(GTK_DROP_DOWN(self->seed_ratio_mode_combo),
                               torrent_get_seed_ratio_mode(json));
    seed_ratio_mode_changed_cb(GTK_DROP_DOWN(self->seed_ratio_mode_combo), NULL, self);
}

static void models_updated(TrgTorrentModel *model, gpointer data)
{
    TrgTorrentPropsDialog *dlg = TRG_TORRENT_PROPS_DIALOG(data);
    GHashTable *ht = get_torrent_table(model);
    gint64 serial = trg_client_get_serial(dlg->client);
    JsonObject *t = NULL;
    TrgTorrentItem *item;
    gboolean exists
        = get_torrent_data(ht, json_array_get_int_element(dlg->targetIds, 0), &t, &item);

    if (exists && dlg->lastJson != t) {
        trg_files_model_update(dlg->filesModel, serial, t, TORRENT_GET_MODE_UPDATE);
        trg_peers_model_update(dlg->peersModel, TRG_TREE_VIEW(dlg->peersTv), serial, t,
                               TORRENT_GET_MODE_UPDATE);
        trg_trackers_model_update(dlg->trackersModel, serial, t, TORRENT_GET_MODE_UPDATE);
        info_page_update(TRG_TORRENT_PROPS_DIALOG(data), t, item);
    }

    gtk_widget_set_sensitive(GTK_WIDGET(dlg->filesTv), exists);
    gtk_widget_set_sensitive(GTK_WIDGET(dlg->trackersTv), exists);
    gtk_widget_set_sensitive(GTK_WIDGET(dlg->peersTv), exists);

    dlg->lastJson = t;
}

static void trg_torrent_props_dialog_constructed(GObject *object)
{
    TrgTorrentPropsDialog *propsDialog = TRG_TORRENT_PROPS_DIALOG(object);
    GtkWindow *window = GTK_WINDOW(object);

    gint rowCount = trg_tree_view_count_selected(TRG_TREE_VIEW(propsDialog->tv));
    TrgPrefs *prefs = trg_client_get_prefs(propsDialog->client);
    propsDialog->show_details
        = trg_prefs_get_int(prefs, TRG_PREFS_KEY_STYLE, TRG_PREFS_GLOBAL) != TRG_STYLE_CLASSIC
        && rowCount == 1;

    gint64 width, height;

    JsonObject *json = NULL;
    TrgTorrentItem *item = NULL;
    GtkNotebook *notebook = GTK_NOTEBOOK(propsDialog->notebook);

    G_OBJECT_CLASS(trg_torrent_props_dialog_parent_class)->constructed(object);

    get_torrent_data(trg_client_get_torrent_table(propsDialog->client),
                     trg_mw_get_selected_torrent_id(propsDialog->parent_win), &json, &item);
    propsDialog->targetIds = build_json_id_array(propsDialog->tv);

    if (rowCount > 1) {
        gchar *windowTitle = g_strdup_printf(_("Multiple (%d) torrent properties"), rowCount);
        gtk_window_set_title(window, windowTitle);
        g_free(windowTitle);
    } else if (rowCount == 1) {
        gtk_window_set_title(window, torrent_get_name(json));
    }

    gtk_window_set_transient_for(window, GTK_WINDOW(propsDialog->parent_win));

    g_signal_connect(G_OBJECT(object), "response", G_CALLBACK(trg_torrent_props_response_cb), NULL);

    if (propsDialog->show_details) {
        gint64 serial = trg_client_get_serial(propsDialog->client);

        /* Information */

        info_page_update(propsDialog, json, item);

        /* Files */

        propsDialog->filesModel = trg_files_model_new();
        propsDialog->filesTv
            = trg_files_tree_view_new(propsDialog->filesModel, propsDialog->parent_win,
                                      propsDialog->client, "TrgFilesTreeView-dialog");
        trg_files_model_update(propsDialog->filesModel, serial, json, TORRENT_GET_MODE_FIRST);
        gtk_notebook_insert_page(notebook, GTK_WIDGET(propsDialog->filesTv),
                                 gtk_label_new(_("Files")), 1);

        /* Peers */

        propsDialog->peersModel = trg_peers_model_new();
        propsDialog->peersTv
            = trg_peers_tree_view_new(prefs, propsDialog->peersModel, "TrgPeersTreeView-dialog");
        trg_peers_model_update(propsDialog->peersModel, TRG_TREE_VIEW(propsDialog->peersTv), serial,
                               json, TORRENT_GET_MODE_FIRST);
        gtk_notebook_insert_page(notebook, GTK_WIDGET(propsDialog->peersTv),
                                 gtk_label_new(_("Peers")), 2);

        /* Trackers */

        propsDialog->trackersModel = trg_trackers_model_new();
        propsDialog->trackersTv
            = trg_trackers_tree_view_new(propsDialog->trackersModel, propsDialog->client,
                                         propsDialog->parent_win, "TrgTrackersTreeView-dialog");
        trg_trackers_tree_view_new_connection(propsDialog->trackersTv, propsDialog->client);
        trg_trackers_model_update(propsDialog->trackersModel, serial, json, TORRENT_GET_MODE_FIRST);
        gtk_notebook_insert_page(notebook, GTK_WIDGET(propsDialog->trackersTv),
                                 gtk_label_new(_("Trackers")), 3);

        g_object_unref(propsDialog->trackersModel);
        g_object_unref(propsDialog->filesModel);
        g_object_unref(propsDialog->peersModel);

        g_signal_connect_object(propsDialog->torrentModel, "update", G_CALLBACK(models_updated),
                                object, G_CONNECT_AFTER);

        propsDialog->lastJson = json;
    } else {
        gtk_notebook_remove_page(notebook, gtk_notebook_page_num(notebook, propsDialog->info_page));
    }

    trg_props_limits_page_load(propsDialog, json);

    if (propsDialog->show_details) {
        TrgPrefs *prefs = trg_client_get_prefs(propsDialog->client);
        if ((width = trg_prefs_get_int(prefs, "dialog-width", TRG_PREFS_GLOBAL)) <= 0
            || (height = trg_prefs_get_int(prefs, "dialog-height", TRG_PREFS_GLOBAL)) <= 0) {
            width = 700;
            height = 600;
        }
    } else {
        width = height = 500;
    }

    gtk_window_set_default_size(window, width, height);
}

static void trg_torrent_props_dialog_dispose(GObject *object)
{
    gtk_widget_dispose_template(GTK_WIDGET(object), TRG_TYPE_TORRENT_PROPS_DIALOG);

    G_OBJECT_CLASS(trg_torrent_props_dialog_parent_class)->dispose(object);
}

static void trg_torrent_props_dialog_finalize(GObject *object)
{
    trg_json_widget_desc_list_free(TRG_TORRENT_PROPS_DIALOG(object)->widgets);
    G_OBJECT_CLASS(trg_torrent_props_dialog_parent_class)->finalize(object);
}

static void trg_torrent_props_dialog_class_init(TrgTorrentPropsDialogClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);
    GtkWidgetClass *widget_class = GTK_WIDGET_CLASS(klass);
    guint i;

    object_class->constructed = trg_torrent_props_dialog_constructed;
    object_class->set_property = trg_torrent_props_dialog_set_property;
    object_class->get_property = trg_torrent_props_dialog_get_property;
    object_class->dispose = trg_torrent_props_dialog_dispose;
    object_class->finalize = trg_torrent_props_dialog_finalize;

    g_object_class_install_property(
        object_class, PROP_TREEVIEW,
        g_param_spec_object("torrent-tree-view", "TrgTorrentTreeView", "TrgTorrentTreeView",
                            TRG_TYPE_TORRENT_TREE_VIEW,
                            G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    g_object_class_install_property(
        object_class, PROP_TORRENT_MODEL,
        g_param_spec_object("torrent-model", "TrgTorrentModel", "TrgTorrentModel",
                            TRG_TYPE_TORRENT_MODEL,
                            G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    g_object_class_install_property(
        object_class, PROP_PARENT_WINDOW,
        g_param_spec_object("parent-window", "Parent window", "Parent window", TRG_TYPE_MAIN_WINDOW,
                            G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    g_object_class_install_property(
        object_class, PROP_CLIENT,
        g_param_spec_pointer("trg-client", "TClient", "Client",
                             G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                 | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    gtk_widget_class_set_template_from_resource(
        widget_class, "/io/github/TransmissionRemoteGtk/ui/trg-torrent-props-dialog.ui");
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, notebook);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, info_page);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog,
                                         bandwidth_priority_combo);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog,
                                         seed_ratio_mode_combo);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, queue_position_label);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, queue_position_spin);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, size_lb);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, have_lb);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, dl_lb);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, ul_lb);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, state_lb);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, date_started_lb);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, eta_lb);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, last_activity_lb);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, error_lb);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, destination_lb);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, hash_lb);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, privacy_lb);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, origin_lb);
    gtk_widget_class_bind_template_child(widget_class, TrgTorrentPropsDialog, comment_buffer);

    /* Declared, for gtk_widget_get_template_child() */
    for (i = 0; i < G_N_ELEMENTS(field_widgets); i++)
        gtk_widget_class_bind_template_child_full(widget_class, field_widgets[i].id, FALSE, 0);

    gtk_widget_class_bind_template_callback(widget_class, seed_ratio_mode_changed_cb);
}

static void trg_torrent_props_dialog_init(TrgTorrentPropsDialog *self)
{
    gtk_widget_init_template(GTK_WIDGET(self));
}

TrgTorrentPropsDialog *trg_torrent_props_dialog_new(GtkWindow *window, TrgTorrentTreeView *treeview,
                                                    TrgTorrentModel *torrentModel,
                                                    TrgClient *client)
{
    return g_object_new(TRG_TYPE_TORRENT_PROPS_DIALOG, "torrent-tree-view", treeview,
                        "torrent-model", torrentModel, "parent-window", window, "trg-client",
                        client, NULL);
}
