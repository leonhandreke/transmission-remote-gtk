/*
 * This file Copyright (C) Mnemosyne LLC
 *
 * This file is licensed by the GPL version 2. Works owned by the
 * Transmission project are granted a special exemption to clause 2(b)
 * so that the bulk of its code can remain under the MIT license.
 * This exemption does not extend to derived works not owned by
 * the Transmission project.
 *
 * $Id: torrent-cell-renderer.c 13388 2012-07-14 19:26:55Z jordan $
 */

/* This was Transmission's torrent cell renderer, modified heavily to work
 * with the TrgTorrentModel instead of libtransmission, and then made a row
 * widget for a GtkListView. */

#include "config.h"

#include <glib/gi18n.h>
#include <gtk/gtk.h>

#include "hig.h"
#include "icons.h"
#include "torrent-row.h"
#include "torrent.h"
#include "trg-client.h"
#include "trg-item.h"
#include "trg-torrent-model.h"
#include "util.h"

#define SMALL_SCALE 0.9
#define BAR_WIDTH   50

struct _TorrentRow {
    GtkWidget parent;

    TrgClient *client;
    gboolean compact;
    TrgTorrentItem *item;
    gulong handler;

    GtkWidget *icon;
    GtkWidget *name;
    GtkWidget *progress;
    GtkWidget *bar;
    GtkWidget *status;

    /* the values of the item, for the text helpers below */
    guint flags;
    guint fileCount;
    gint64 uploadedEver;
    gint64 sizeWhenDone;
    gint64 totalSize;
    gint64 downloaded;
    gint64 haveValid;
    gint64 haveUnchecked;
    gint64 upSpeed;
    gint64 downSpeed;
    gint64 peersFromUs;
    gint64 webSeedsToUs;
    gint64 peersToUs;
    gint64 connected;
    gint64 eta;
    gint64 error;
    gint64 seedRatioMode;
    gdouble done;
    gdouble metadataPercentComplete;
    gdouble ratio;
    gdouble seedRatioLimit;
    JsonObject *json;
};

G_DEFINE_FINAL_TYPE(TorrentRow, torrent_row, GTK_TYPE_WIDGET)

static gboolean getSeedRatio(TorrentRow *r, gdouble *ratio)
{
    if ((r->seedRatioMode == 0) && (trg_client_get_seed_ratio_limited(r->client) == TRUE)) {
        *ratio = trg_client_get_seed_ratio_limit(r->client);
        return TRUE;
    } else if (r->seedRatioMode == 1) {
        *ratio = r->seedRatioLimit;
        return TRUE;
    }

    return FALSE;
}

static void getProgressString(GString *gstr, TorrentRow *r)
{
    const gint64 haveTotal = r->haveUnchecked + r->haveValid;
    const int isSeed = r->haveValid >= r->totalSize;
    char buf1[32], buf2[32], buf3[32], buf4[32], buf5[32], buf6[32];
    double seedRatio;
    const gboolean hasSeedRatio = getSeedRatio(r, &seedRatio);

    if (r->flags & TORRENT_FLAG_DOWNLOADING) { /* downloading */
        g_string_append_printf(gstr,
                               /* %1$s is how much we've got,
                                  %2$s is how much we'll have when done,
                                  %3$s%% is a percentage of the two */
                               _("%1$s of %2$s (%3$s)"), tr_strlsize(buf1, haveTotal, sizeof(buf1)),
                               tr_strlsize(buf2, r->sizeWhenDone, sizeof(buf2)),
                               tr_strlpercent(buf3, r->done, sizeof(buf3)));
    } else if (!isSeed) { /* Partial seed */
        if (hasSeedRatio) {
            g_string_append_printf(gstr,
                                   _("%1$s of %2$s (%3$s), uploaded %4$s (Ratio: %5$s Goal: %6$s)"),
                                   tr_strlsize(buf1, haveTotal, sizeof(buf1)),
                                   tr_strlsize(buf2, r->totalSize, sizeof(buf2)),
                                   tr_strlpercent(buf3, r->done, sizeof(buf3)),
                                   tr_strlsize(buf4, r->uploadedEver, sizeof(buf4)),
                                   tr_strlratio(buf5, r->ratio, sizeof(buf5)),
                                   tr_strlratio(buf6, seedRatio, sizeof(buf6)));
        } else {
            g_string_append_printf(gstr, _("%1$s of %2$s (%3$s), uploaded %4$s (Ratio: %5$s)"),
                                   tr_strlsize(buf1, haveTotal, sizeof(buf1)),
                                   tr_strlsize(buf2, r->totalSize, sizeof(buf2)),
                                   tr_strlpercent(buf3, r->done, sizeof(buf3)),
                                   tr_strlsize(buf4, r->uploadedEver, sizeof(buf4)),
                                   tr_strlratio(buf5, r->ratio, sizeof(buf5)));
        }
    } else { /* seeding */

        if (hasSeedRatio) {
            g_string_append_printf(gstr, _("%1$s, uploaded %2$s (Ratio: %3$s Goal: %4$s)"),
                                   tr_strlsize(buf1, r->totalSize, sizeof(buf1)),
                                   tr_strlsize(buf2, r->uploadedEver, sizeof(buf2)),
                                   tr_strlratio(buf3, r->ratio, sizeof(buf3)),
                                   tr_strlratio(buf4, seedRatio, sizeof(buf4)));
        } else {
            g_string_append_printf(gstr,
                                   /* %1$s is the torrent's total size,
                                      %2$s is how much we've uploaded,
                                      %3$s is our upload-to-download ratio */
                                   _("%1$s, uploaded %2$s (Ratio: %3$s)"),
                                   tr_strlsize(buf1, r->sizeWhenDone, sizeof(buf1)),
                                   tr_strlsize(buf2, r->uploadedEver, sizeof(buf2)),
                                   tr_strlratio(buf3, r->ratio, sizeof(buf3)));
        }
    }

    /* add time when downloading */
    if ((r->flags & TORRENT_FLAG_DOWNLOADING)
        || (hasSeedRatio && (r->flags & TORRENT_FLAG_SEEDING))) {
        gint64 eta = r->eta;
        g_string_append(gstr, " - ");
        if (eta < 0)
            g_string_append(gstr, _("Remaining time unknown"));
        else {
            char timestr[128];
            tr_strltime_long(timestr, eta, sizeof(timestr));
            /* time remaining */
            g_string_append_printf(gstr, _("%s remaining"), timestr);
        }
    }
}

static char *getShortTransferString(TorrentRow *r, char *buf, size_t buflen)
{
    char downStr[32], upStr[32];
    const gboolean haveMeta = r->fileCount > 0;
    const gboolean haveUp = haveMeta && r->peersFromUs > 0;
    const gboolean haveDown = haveMeta && r->peersToUs > 0;

    if (haveDown)
        tr_formatter_speed_KBps(downStr, r->downSpeed / speed_K, sizeof(downStr));
    if (haveUp)
        tr_formatter_speed_KBps(upStr, r->upSpeed / speed_K, sizeof(upStr));

    if (haveDown && haveUp)
        /* 1==down arrow, 2==down speed, 3==up arrow, 4==down speed */
        g_snprintf(buf, buflen, _("%1$s %2$s, %3$s %4$s"), GTR_UNICODE_DOWN, downStr,
                   GTR_UNICODE_UP, upStr);
    else if (haveDown)
        /* bandwidth speed + unicode arrow */
        g_snprintf(buf, buflen, _("%1$s %2$s"), GTR_UNICODE_DOWN, downStr);
    else if (haveUp)
        /* bandwidth speed + unicode arrow */
        g_snprintf(buf, buflen, _("%1$s %2$s"), GTR_UNICODE_UP, upStr);
    /*else if( st->isStalled )
       g_strlcpy( buf, _( "Stalled" ), buflen ); */
    else if (haveMeta)
        g_strlcpy(buf, _("Idle"), buflen);
    else
        *buf = '\0';

    return buf;
}

static void getShortStatusString(GString *gstr, TorrentRow *r)
{
    guint flags = r->flags;

    if (flags & TORRENT_FLAG_PAUSED) {
        g_string_append(gstr, (flags & TORRENT_FLAG_COMPLETE) ? _("Finished") : _("Paused"));
    } else if (flags & TORRENT_FLAG_WAITING_CHECK) {
        g_string_append(gstr, _("Queued for verification"));
    } else if (flags & TORRENT_FLAG_DOWNLOADING_WAIT) {
        g_string_append(gstr, _("Queued for download"));
    } else if (flags & TORRENT_FLAG_SEEDING_WAIT) {
        g_string_append(gstr, _("Queued for seeding"));
    } else if (flags & TORRENT_FLAG_CHECKING) {
        char buf1[32];
        g_string_append_printf(gstr, _("Verifying data (%1$s tested)"),
                               tr_strlpercent(buf1, r->done, sizeof(buf1)));
    } else if ((flags & TORRENT_FLAG_DOWNLOADING) || (flags & TORRENT_FLAG_SEEDING)) {
        char buf[512];
        if (flags & ~TORRENT_FLAG_DOWNLOADING) {
            tr_strlratio(buf, r->ratio, sizeof(buf));
            g_string_append_printf(gstr, _("Ratio %s"), buf);
            g_string_append(gstr, ", ");
        }
        getShortTransferString(r, buf, sizeof(buf));
        g_string_append(gstr, buf);
    }
}

static void getStatusString(GString *gstr, TorrentRow *r)
{
    char buf[256];
    const gchar *errstr;

    if (r->error) {
        errstr = torrent_get_errorstr(r->json);
        switch (r->error) {
        case 0: /* OK */
            break;
        case 1: /* Tracking Warning */
            g_string_append_printf(gstr, _("Tracker gave a warning: \"%s\""), errstr);
            break;
        case 2: /* Tracker Error */
            g_string_append_printf(gstr, _("Tracker gave an error: \"%s\""), errstr);
            break;
        case 3: /* Local Error */
            g_string_append_printf(gstr, _("Error: \"%s\""), errstr);
            break;
        }
    } else if ((r->flags & TORRENT_FLAG_PAUSED) || (r->flags & TORRENT_FLAG_WAITING_CHECK)
               || (r->flags & TORRENT_FLAG_CHECKING) || (r->flags & TORRENT_FLAG_DOWNLOADING_WAIT)
               || (r->flags & TORRENT_FLAG_SEEDING_WAIT)) {
        getShortStatusString(gstr, r);
    } else if (r->flags & TORRENT_FLAG_DOWNLOADING) {
        if (r->fileCount > 0) {
            g_string_append_printf(gstr,
                                   ngettext("Downloading from %1$" G_GUINT64_FORMAT
                                            " of %2$" G_GUINT64_FORMAT " connected peer",
                                            "Downloading from %1$" G_GUINT64_FORMAT
                                            " of %2$" G_GUINT64_FORMAT " connected peers",
                                            r->webSeedsToUs + r->peersToUs),
                                   r->webSeedsToUs + r->peersToUs, r->webSeedsToUs + r->connected);
        } else {
            g_string_append_printf(
                gstr,
                ngettext("Downloading metadata from %1$" G_GUINT64_FORMAT " peer (%2$s done)",
                         "Downloading metadata from %1$" G_GUINT64_FORMAT " peers (%2$s done)",
                         r->connected + r->webSeedsToUs),
                r->connected + r->webSeedsToUs,
                tr_strlpercent(buf, r->metadataPercentComplete, sizeof(buf)));
        }
    } else if (r->flags & TORRENT_FLAG_SEEDING) {
        g_string_append_printf(gstr,
                               ngettext("Seeding to %1$" G_GUINT64_FORMAT " of %2$" G_GUINT64_FORMAT
                                        " connected peer",
                                        "Seeding to %1$" G_GUINT64_FORMAT " of %2$" G_GUINT64_FORMAT
                                        " connected peers",
                                        r->connected),
                               r->peersFromUs, r->connected);
    }

    if ((r->flags & ~TORRENT_FLAG_WAITING_CHECK) && (r->flags & ~TORRENT_FLAG_CHECKING)
        && (r->flags & ~TORRENT_FLAG_DOWNLOADING_WAIT) && (r->flags & ~TORRENT_FLAG_SEEDING_WAIT)
        && (r->flags & ~TORRENT_FLAG_PAUSED)) {
        getShortTransferString(r, buf, sizeof(buf));
        if (*buf)
            g_string_append_printf(gstr, " - %s", buf);
    }
}

/***
****
***/

static GIcon *get_icon(TorrentRow *r)
{
    const char *mime_type;

    if (r->fileCount == 0)
        mime_type = UNKNOWN_MIME_TYPE;
    else if (r->fileCount > 1)
        mime_type = DIRECTORY_MIME_TYPE;
    /*else if( strchr( info->files[0].name, '/' ) != NULL )
       mime_type = DIRECTORY_MIME_TYPE;
       else
       mime_type = gtr_get_mime_type_from_filename( info->files[0].name ); */
    else
        mime_type = FILE_MIME_TYPE;

    // return NULL;
    return gtr_get_mime_type_icon(mime_type);
}

/***
****
***/

static double get_percent_done(TorrentRow *r, gboolean *seed)
{
    gdouble d;

    if ((r->flags & TORRENT_FLAG_SEEDING) && getSeedRatio(r, &d)) {
        *seed = TRUE;
        const gint64 baseline = r->downloaded ? r->downloaded : r->sizeWhenDone;
        const gint64 goal = baseline * r->seedRatioLimit;
        const gint64 bytesLeft = goal > r->uploadedEver ? goal - r->uploadedEver : 0;
        float seedRatioPercentDone = (gdouble)(goal - bytesLeft) / goal;

        d = MAX(0.0, seedRatioPercentDone * 100.0);
    } else {
        *seed = FALSE;
        d = MAX(0.0, r->done);
    }

    return d;
}

static void torrent_row_set_text_style(GtkWidget *label, TorrentRow *r)
{
    /* errors in red, paused torrents dimmed */
    if (r->error)
        gtk_widget_add_css_class(label, "error");
    else
        gtk_widget_remove_css_class(label, "error");

    gtk_widget_set_opacity(label, !r->error && (r->flags & TORRENT_FLAG_PAUSED) ? 0.55 : 1.0);
}

static void torrent_row_refresh(TorrentRow *r)
{
    TrgTorrentItem *item = r->item;
    const gboolean active = (r->flags & ~TORRENT_FLAG_PAUSED)
        && (r->flags & ~TORRENT_FLAG_DOWNLOADING_WAIT) && (r->flags & ~TORRENT_FLAG_SEEDING_WAIT);
    gboolean sensitive, seed;
    gdouble percentDone;
    GString *gstr;
    GIcon *icon;

    if (!item)
        return;

    r->flags = trg_item_get_uint(item, TORRENT_COLUMN_FLAGS);
    r->fileCount = trg_item_get_uint(item, TORRENT_COLUMN_FILECOUNT);
    r->uploadedEver = trg_item_get_int64(item, TORRENT_COLUMN_UPLOADED);
    r->sizeWhenDone = trg_item_get_int64(item, TORRENT_COLUMN_SIZEWHENDONE);
    r->totalSize = trg_item_get_int64(item, TORRENT_COLUMN_TOTALSIZE);
    r->downloaded = trg_item_get_int64(item, TORRENT_COLUMN_DOWNLOADED);
    r->haveValid = trg_item_get_int64(item, TORRENT_COLUMN_HAVE_VALID);
    r->haveUnchecked = trg_item_get_int64(item, TORRENT_COLUMN_HAVE_UNCHECKED);
    r->upSpeed = trg_item_get_int64(item, TORRENT_COLUMN_UPSPEED);
    r->downSpeed = trg_item_get_int64(item, TORRENT_COLUMN_DOWNSPEED);
    r->peersFromUs = trg_item_get_int64(item, TORRENT_COLUMN_PEERS_FROM_US);
    r->webSeedsToUs = trg_item_get_int64(item, TORRENT_COLUMN_WEB_SEEDS_TO_US);
    r->peersToUs = trg_item_get_int64(item, TORRENT_COLUMN_PEERS_TO_US);
    r->connected = trg_item_get_int64(item, TORRENT_COLUMN_PEERS_CONNECTED);
    r->eta = trg_item_get_int64(item, TORRENT_COLUMN_ETA);
    r->error = trg_item_get_int64(item, TORRENT_COLUMN_ERROR);
    r->seedRatioMode = trg_item_get_int64(item, TORRENT_COLUMN_SEED_RATIO_MODE);
    r->done = trg_item_get_double(item, TORRENT_COLUMN_PERCENTDONE);
    r->metadataPercentComplete = trg_item_get_double(item, TORRENT_COLUMN_METADATAPERCENTCOMPLETE);
    r->ratio = trg_item_get_double(item, TORRENT_COLUMN_RATIO);
    r->seedRatioLimit = trg_item_get_double(item, TORRENT_COLUMN_SEED_RATIO_LIMIT);
    r->json = trg_torrent_item_get_json(item);

    sensitive = active || r->error;
    percentDone = get_percent_done(r, &seed);

    icon = get_icon(r);
    gtk_image_set_from_gicon(GTK_IMAGE(r->icon), icon);
    g_object_unref(icon);
    gtk_widget_set_sensitive(r->icon, sensitive);

    gtk_label_set_text(GTK_LABEL(r->name), trg_item_get_string(item, TORRENT_COLUMN_NAME));
    torrent_row_set_text_style(r->name, r);

    gtk_progress_bar_set_fraction(GTK_PROGRESS_BAR(r->bar), CLAMP(percentDone, 0.0, 100.0) / 100.0);
    gtk_widget_set_sensitive(r->bar, sensitive);

    gstr = g_string_new(NULL);

    if (r->compact) {
        gchar *text = g_strdup_printf(C_("progress bar label", "%d %%"), (gint)percentDone);
        gtk_progress_bar_set_text(GTK_PROGRESS_BAR(r->bar), text);
        g_free(text);

        getShortStatusString(gstr, r);
    } else {
        getProgressString(gstr, r);
        gtk_label_set_text(GTK_LABEL(r->progress), gstr->str);
        torrent_row_set_text_style(r->progress, r);

        g_string_truncate(gstr, 0);
        getStatusString(gstr, r);
    }

    gtk_label_set_text(GTK_LABEL(r->status), gstr->str);
    torrent_row_set_text_style(r->status, r);

    g_string_free(gstr, TRUE);
}

/* An update sets the update serial last, with notifications frozen, so all
 * the values have changed by the time this is notified. */
static void torrent_row_item_updated(GObject *item, GParamSpec *pspec, TorrentRow *r)
{
    torrent_row_refresh(r);
}

void torrent_row_set_item(TorrentRow *r, TrgTorrentItem *item)
{
    if (r->item == item)
        return;

    if (r->item) {
        g_clear_signal_handler(&r->handler, r->item);
        g_clear_object(&r->item);
    }

    if (item) {
        gchar *signal = g_strconcat(
            "notify::",
            g_param_spec_get_name(trg_item_get_column_pspec(item, TORRENT_COLUMN_UPDATESERIAL)),
            NULL);

        r->item = g_object_ref(item);
        r->handler = g_signal_connect(item, signal, G_CALLBACK(torrent_row_item_updated), r);
        g_free(signal);

        torrent_row_refresh(r);
    }
}

static GtkWidget *torrent_row_label_new(gdouble scale, gboolean bold)
{
    GtkWidget *label = gtk_label_new(NULL);
    PangoAttrList *attrs = pango_attr_list_new();

    if (scale != 1.0)
        pango_attr_list_insert(attrs, pango_attr_scale_new(scale));
    if (bold)
        pango_attr_list_insert(attrs, pango_attr_weight_new(PANGO_WEIGHT_BOLD));

    gtk_label_set_attributes(GTK_LABEL(label), attrs);
    pango_attr_list_unref(attrs);

    gtk_label_set_xalign(GTK_LABEL(label), 0);
    gtk_label_set_ellipsize(GTK_LABEL(label), PANGO_ELLIPSIZE_END);
    gtk_label_set_single_line_mode(GTK_LABEL(label), TRUE);

    return label;
}

static void torrent_row_build(TorrentRow *r)
{
    GtkWidget *w = GTK_WIDGET(r);

    r->icon = gtk_image_new();
    gtk_widget_set_parent(r->icon, w);

    r->bar = gtk_progress_bar_new();
    gtk_widget_set_valign(r->bar, GTK_ALIGN_CENTER);

    if (r->compact) {
        /* icon, name, status, a short progress bar */
        gtk_image_set_icon_size(GTK_IMAGE(r->icon), GTK_ICON_SIZE_NORMAL);

        r->name = torrent_row_label_new(1.0, FALSE);
        gtk_widget_set_hexpand(r->name, TRUE);
        gtk_widget_set_parent(r->name, w);

        r->status = torrent_row_label_new(SMALL_SCALE, FALSE);
        gtk_label_set_ellipsize(GTK_LABEL(r->status), PANGO_ELLIPSIZE_NONE);
        gtk_widget_set_parent(r->status, w);

        gtk_progress_bar_set_show_text(GTK_PROGRESS_BAR(r->bar), TRUE);
        gtk_widget_set_size_request(r->bar, BAR_WIDTH, -1);
        gtk_widget_set_parent(r->bar, w);
    } else {
        /* icon, and then name, progress, progress bar and status lines */
        GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);

        gtk_image_set_icon_size(GTK_IMAGE(r->icon), GTK_ICON_SIZE_LARGE);
        gtk_widget_set_valign(r->icon, GTK_ALIGN_CENTER);

        r->name = torrent_row_label_new(1.0, TRUE);
        gtk_box_append(GTK_BOX(box), r->name);

        r->progress = torrent_row_label_new(SMALL_SCALE, FALSE);
        gtk_box_append(GTK_BOX(box), r->progress);

        gtk_widget_set_margin_top(r->bar, GUI_PAD_SMALL);
        gtk_widget_set_margin_bottom(r->bar, GUI_PAD_SMALL);
        gtk_box_append(GTK_BOX(box), r->bar);

        r->status = torrent_row_label_new(SMALL_SCALE, FALSE);
        gtk_box_append(GTK_BOX(box), r->status);

        gtk_widget_set_hexpand(box, TRUE);
        gtk_widget_set_parent(box, w);
    }
}

static void torrent_row_dispose(GObject *object)
{
    TorrentRow *r = TORRENT_ROW(object);
    GtkWidget *child;

    torrent_row_set_item(r, NULL);

    while ((child = gtk_widget_get_first_child(GTK_WIDGET(object))))
        gtk_widget_unparent(child);

    G_OBJECT_CLASS(torrent_row_parent_class)->dispose(object);
}

static void torrent_row_class_init(TorrentRowClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);
    GtkWidgetClass *widget_class = GTK_WIDGET_CLASS(klass);

    object_class->dispose = torrent_row_dispose;

    gtk_widget_class_set_layout_manager_type(widget_class, GTK_TYPE_BOX_LAYOUT);
    gtk_widget_class_set_css_name(widget_class, "torrent-row");
}

static void torrent_row_init(TorrentRow *self)
{
    GtkLayoutManager *layout = gtk_widget_get_layout_manager(GTK_WIDGET(self));

    gtk_box_layout_set_spacing(GTK_BOX_LAYOUT(layout), GUI_PAD);
    gtk_widget_set_margin_top(GTK_WIDGET(self), 2);
    gtk_widget_set_margin_bottom(GTK_WIDGET(self), 2);
}

GtkWidget *torrent_row_new(TrgClient *client, gboolean compact)
{
    TorrentRow *r = g_object_new(TORRENT_TYPE_ROW, NULL);

    r->client = client;
    r->compact = compact;
    torrent_row_build(r);

    return GTK_WIDGET(r);
}
