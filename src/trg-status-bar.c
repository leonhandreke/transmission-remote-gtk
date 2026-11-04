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
#include <glib/gprintf.h>
#include <gtk/gtk.h>
#include <pango/pango.h>

#include "json.h"
#include "requests.h"
#include "session-get.h"
#include "trg-main-window.h"
#include "trg-prefs.h"
#include "trg-status-bar.h"
#include "trg-torrent-model.h"
#include "util.h"

/* A subclass of GtkBox which contains a status label on the left.
 * Free space indicator on left-right.
 * Speed (including limits if in use) label on right-right.
 *
 * Status and speed labels should be updated on every torrent-get using
 * trg_status_bar_update. Free space is updated with trg_status_bar_session_update.
 *
 * There's a signal in TrgClient for session updates, connected into the
 * main window, which calls this. Session updates happen every 10 torrent-get updates.
 */

struct _TrgStatusBar {
    GtkBox parent;

    GtkWidget *speed_lbl;
    GtkWidget *turtle_image;
    GtkWidget *free_lbl;
    GtkWidget *info_lbl;
    TrgClient *client;
    TrgMainWindow *win;
};

G_DEFINE_TYPE(TrgStatusBar, trg_status_bar, GTK_TYPE_BOX)

void trg_status_bar_clear_indicators(TrgStatusBar *sb)
{
    gtk_label_set_text(GTK_LABEL(sb->free_lbl), "");
    gtk_label_set_text(GTK_LABEL(sb->speed_lbl), "");
}

void trg_status_bar_reset(TrgStatusBar *sb)
{
    trg_status_bar_clear_indicators(sb);
    gtk_label_set_text(GTK_LABEL(sb->info_lbl), _("Disconnected"));
    gtk_widget_set_visible(sb->turtle_image, FALSE);
}

static void turtle_toggle(GtkGestureClick *gesture, gint n_press, gdouble x, gdouble y,
                          gpointer data)
{
    TrgStatusBar *sb = TRG_STATUS_BAR(data);
    JsonNode *req = session_set();
    JsonObject *args = node_get_arguments(req);
    const gchar *iconName;
    gboolean altSpeedOn;

    iconName = gtk_image_get_icon_name(GTK_IMAGE(sb->turtle_image));
    altSpeedOn = g_strcmp0(iconName, "alt-speed-on") == 0;

    gtk_image_set_from_icon_name(GTK_IMAGE(sb->turtle_image),
                                 altSpeedOn ? "alt-speed-off" : "alt-speed-on");
    json_object_set_boolean_member(args, SGET_ALT_SPEED_ENABLED, !altSpeedOn);

    dispatch_rpc_async(sb->client, req, on_session_set, sb->win);
}

static void trg_status_bar_dispose(GObject *object)
{
    gtk_widget_dispose_template(GTK_WIDGET(object), TRG_TYPE_STATUS_BAR);

    G_OBJECT_CLASS(trg_status_bar_parent_class)->dispose(object);
}

static void trg_status_bar_class_init(TrgStatusBarClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);
    GtkWidgetClass *widget_class = GTK_WIDGET_CLASS(klass);

    object_class->dispose = trg_status_bar_dispose;

    gtk_widget_class_set_template_from_resource(
        widget_class, "/io/github/TransmissionRemoteGtk/ui/trg-status-bar.ui");
    gtk_widget_class_bind_template_child(widget_class, TrgStatusBar, info_lbl);
    gtk_widget_class_bind_template_child(widget_class, TrgStatusBar, free_lbl);
    gtk_widget_class_bind_template_child(widget_class, TrgStatusBar, speed_lbl);
    gtk_widget_class_bind_template_child(widget_class, TrgStatusBar, turtle_image);
    gtk_widget_class_bind_template_callback(widget_class, turtle_toggle);
}

static void trg_status_bar_init(TrgStatusBar *self)
{
    gtk_widget_init_template(GTK_WIDGET(self));
}

void trg_status_bar_push_connection_msg(TrgStatusBar *sb, const gchar *msg)
{
    gtk_label_set_text(GTK_LABEL(sb->info_lbl), msg);
}

static void trg_status_bar_set_connected_label(TrgStatusBar *sb, JsonObject *session,
                                               TrgClient *client)
{
    TrgPrefs *prefs = trg_client_get_prefs(client);

    gchar *profileName
        = trg_prefs_get_string(prefs, TRG_PREFS_KEY_PROFILE_NAME, TRG_PREFS_CONNECTION);
    gchar *statusMsg = g_strdup_printf(_("Connected: %s :: %s"), profileName,
                                       session_get_version_string(session));

    trg_status_bar_push_connection_msg(sb, statusMsg);

    g_free(profileName);
    g_free(statusMsg);
}

void trg_status_bar_connect(TrgStatusBar *sb, JsonObject *session, TrgClient *client)
{

    trg_status_bar_set_connected_label(sb, session, client);
    gtk_label_set_text(GTK_LABEL(sb->speed_lbl), _("Updating torrents..."));
}

void trg_status_bar_session_update(TrgStatusBar *sb, JsonObject *session)
{
    gint64 free = session_get_download_dir_free_space(session);
    gboolean altSpeedEnabled = session_get_alt_speed_enabled(session);
    gchar freeSpace[64];

    if (free >= 0) {
        gchar *freeSpaceString;
        trg_strlsize(freeSpace, free);
        freeSpaceString = g_strdup_printf(_("Free space: %s"), freeSpace);
        gtk_label_set_text(GTK_LABEL(sb->free_lbl), freeSpaceString);
        g_free(freeSpaceString);
    } else {
        gtk_label_set_text(GTK_LABEL(sb->free_lbl), "");
    }

    gtk_image_set_from_icon_name(GTK_IMAGE(sb->turtle_image),
                                 altSpeedEnabled ? "alt-speed-on" : "alt-speed-off");
    gtk_widget_set_tooltip_text(sb->turtle_image,
                                altSpeedEnabled ? _("Disable alternate speed limits")
                                                : _("Enable alternate speed limits"));
    gtk_widget_set_visible(sb->turtle_image, TRUE);
}

void trg_status_bar_update_speed(TrgStatusBar *sb, trg_torrent_model_update_stats *stats,
                                 TrgClient *client)
{
    JsonObject *session = trg_client_get_session(client);
    gboolean altLimits = session_get_speed_limit_alt_enabled(session);
    gchar *speedText;
    gint64 uplimitraw, downlimitraw;
    gchar downRateTotalString[32], upRateTotalString[32];
    gchar uplimit[64], downlimit[64];

    if (altLimits) {
        downlimitraw = session_get_alt_speed_limit_down(session);
        uplimitraw = session_get_alt_speed_limit_up(session);
    } else {
        if (session_get_speed_limit_down_enabled(session))
            downlimitraw = session_get_speed_limit_down(session);
        else
            downlimitraw = -1;
        if (session_get_speed_limit_up_enabled(session))
            uplimitraw = session_get_speed_limit_up(session);
        else
            uplimitraw = -1;
    }

    trg_strlspeed(downRateTotalString, stats->downRateTotal / disk_K);
    trg_strlspeed(upRateTotalString, stats->upRateTotal / disk_K);

    if (uplimitraw >= 0) {
        gchar uplimitstring[32];
        trg_strlspeed(uplimitstring, uplimitraw);
        g_snprintf(uplimit, sizeof(uplimit), _(" (Limit: %s)"), uplimitstring);
    }

    if (downlimitraw >= 0) {
        gchar downlimitstring[32];
        trg_strlspeed(downlimitstring, downlimitraw);
        g_snprintf(downlimit, sizeof(downlimit), _(" (Limit: %s)"), downlimitstring);
    }

    speedText = g_strdup_printf(_("Down: %s%s, Up: %s%s"), downRateTotalString,
                                downlimitraw >= 0 ? downlimit : "", upRateTotalString,
                                uplimitraw >= 0 ? uplimit : "");

    gtk_label_set_text(GTK_LABEL(sb->speed_lbl), speedText);

    g_free(speedText);
}

void trg_status_bar_update(TrgStatusBar *sb, trg_torrent_model_update_stats *stats,
                           TrgClient *client)
{
    trg_status_bar_set_connected_label(sb, trg_client_get_session(client), client);
    trg_status_bar_update_speed(sb, stats, client);
}

const gchar *trg_status_bar_get_speed_text(TrgStatusBar *sb)
{
    return gtk_label_get_text(GTK_LABEL(sb->speed_lbl));
}

/* The main window has the status bar in its template, so this is set later */
void trg_status_bar_set_client(TrgStatusBar *sb, TrgMainWindow *win, TrgClient *client)
{
    sb->client = client;
    sb->win = win;
}
