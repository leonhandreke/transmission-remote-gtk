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

/* Many of these functions are taken from the Transmission Project. */

#include "config.h"

#include <limits.h>
#include <math.h>
#include <stdlib.h>
#include <string.h>

#include <glib-object.h>
#include <glib.h>
#include <glib/gi18n.h>
#include <glib/gprintf.h>
#include <gtk/gtk.h>
#include <json-glib/json-glib.h>

#include "util.h"

/***
**** The code for formatting size and speeds, taken from Transmission.
***/

const int disk_K = 1024;
const char *disk_K_str = N_("KiB");
const char *disk_M_str = N_("MiB");
const char *disk_G_str = N_("GiB");
const char *disk_T_str = N_("TiB");

const int speed_K = 1024;
const char *speed_K_str = N_("KiB/s");
const char *speed_M_str = N_("MiB/s");
const char *speed_G_str = N_("GiB/s");
const char *speed_T_str = N_("TiB/s");

struct formatter_unit {
    char *name;
    gint64 value;
};

struct formatter_units {
    struct formatter_unit units[4];
};

enum {
    TR_FMT_KB,
    TR_FMT_MB,
    TR_FMT_GB,
    TR_FMT_TB
};

static void formatter_init(struct formatter_units *units, unsigned int kilo, const char *kb,
                           const char *mb, const char *gb, const char *tb)
{
    guint64 value = kilo;
    units->units[TR_FMT_KB].name = g_strdup(kb);
    units->units[TR_FMT_KB].value = value;

    value *= kilo;
    units->units[TR_FMT_MB].name = g_strdup(mb);
    units->units[TR_FMT_MB].value = value;

    value *= kilo;
    units->units[TR_FMT_GB].name = g_strdup(gb);
    units->units[TR_FMT_GB].value = value;

    value *= kilo;
    units->units[TR_FMT_TB].name = g_strdup(tb);
    units->units[TR_FMT_TB].value = value;
}

static char *formatter_get_size_str(const struct formatter_units *u, char *buf, gint64 bytes,
                                    size_t buflen)
{
    int precision;
    double value;
    const char *units;
    const struct formatter_unit *unit;

    if (bytes < u->units[1].value)
        unit = &u->units[0];
    else if (bytes < u->units[2].value)
        unit = &u->units[1];
    else if (bytes < u->units[3].value)
        unit = &u->units[2];
    else
        unit = &u->units[3];

    value = (double)bytes / unit->value;
    units = unit->name;
    if (unit->value == 1)
        precision = 0;
    else if (value < 100)
        precision = 2;
    else
        precision = 1;
    g_snprintf(buf, buflen, "%.*f %s", precision, value, units);
    return buf;
}

static struct formatter_units size_units;

void tr_formatter_size_init(unsigned int kilo, const char *kb, const char *mb, const char *gb,
                            const char *tb)
{
    formatter_init(&size_units, kilo, kb, mb, gb, tb);
}

char *tr_formatter_size_B(char *buf, gint64 bytes, size_t buflen)
{
    return formatter_get_size_str(&size_units, buf, bytes, buflen);
}

static struct formatter_units speed_units;

unsigned int tr_speed_K = 0u;

void tr_formatter_speed_init(unsigned int kilo, const char *kb, const char *mb, const char *gb,
                             const char *tb)
{
    tr_speed_K = kilo;
    formatter_init(&speed_units, kilo, kb, mb, gb, tb);
}

char *tr_formatter_speed_KBps(char *buf, double KBps, size_t buflen)
{
    const double K = speed_units.units[TR_FMT_KB].value;
    double speed = KBps;

    if (speed <= 999.95) /* 0.0 KB to 999.9 KB */
        g_snprintf(buf, buflen, "%d %s", (int)speed, speed_units.units[TR_FMT_KB].name);
    else {
        speed /= K;
        if (speed <= 99.995) /* 0.98 MB to 99.99 MB */
            g_snprintf(buf, buflen, "%.2f %s", speed, speed_units.units[TR_FMT_MB].name);
        else if (speed <= 999.95) /* 100.0 MB to 999.9 MB */
            g_snprintf(buf, buflen, "%.1f %s", speed, speed_units.units[TR_FMT_MB].name);
        else {
            speed /= K;
            g_snprintf(buf, buflen, "%.1f %s", speed, speed_units.units[TR_FMT_GB].name);
        }
    }

    return buf;
}

/* URL checkers. */

gboolean is_magnet(const gchar *string)
{
    return g_str_has_prefix(string, "magnet:");
}

gboolean is_url(const gchar *string)
{
    /* return g_regex_match_simple ("^(([^:/?#]+):)?(//([^/?#]*))?([^?#]*)(\?([^#]*))?(#(.*))?",
     * string, 0, 0); */
    return g_regex_match_simple("^http[s]?://", string, 0, 0);
}

/*
 * Glib-ish Utility functions.
 */

gchar *trg_base64encode(const gchar *filename, GError **error)
{
    GMappedFile *mf = g_mapped_file_new(filename, FALSE, error);
    gchar *b64out = NULL;

    /* Should not be possible */
    g_assert(error != NULL);

    if (*error) {
        return NULL;
    } else {
        b64out = g_base64_encode((guchar *)g_mapped_file_get_contents(mf),
                                 g_mapped_file_get_length(mf));
    }

    g_mapped_file_unref(mf);

    return b64out;
}

gchar *trg_gregex_get_first(GRegex *rx, const gchar *src)
{
    GMatchInfo *mi = NULL;
    gchar *dst = NULL;
    g_regex_match(rx, src, 0, &mi);
    if (mi) {
        dst = g_match_info_fetch(mi, 1);
        g_match_info_free(mi);
    }
    return dst;
}

GRegex *trg_uri_host_regex_new(void)
{
    return g_regex_new(
        "^[^:/?#]+:?//(?:www\\.|torrent\\.|torrents\\.|tracker\\.|\\d+\\.)?([^/?#:]*)",
        G_REGEX_OPTIMIZE, 0, NULL);
}

void g_str_slist_free(GSList *list)
{
    g_slist_free_full(list, (GDestroyNotify)g_free);
}

void rm_trailing_slashes(gchar *str)
{
    int i, len;

    if (!str)
        return;

    if ((len = strlen(str)) < 1)
        return;

    for (i = strlen(str) - 1; str[i]; i--) {
        if (str[i] == '/')
            str[i] = '\0';
        else
            return;
    }
}

/* Working with torrents.. */

void add_file_id_to_array(JsonObject *args, const gchar *key, gint index)
{
    JsonArray *array;
    if (json_object_has_member(args, key)) {
        array = json_object_get_array_member(args, key);
    } else {
        array = json_array_new();
        json_object_set_array_member(args, key, array);
    }
    json_array_add_int_element(array, index);
}

/* GTK utilities. */

GtkWidget *gtr_combo_box_new_enum(const char *text_1, ...)
{
    GtkWidget *w;
    GtkCellRenderer *r;
    GtkListStore *store;
    va_list vl;
    const char *text;
    va_start(vl, text_1);

    store = gtk_list_store_new(2, G_TYPE_INT, G_TYPE_STRING);

    text = text_1;
    if (text != NULL)
        do {
            const int val = va_arg(vl, int);
            gtk_list_store_insert_with_values(store, NULL, INT_MAX, 0, val, 1, text, -1);
            text = va_arg(vl, const char *);
        } while (text != NULL);

    w = gtk_combo_box_new_with_model(GTK_TREE_MODEL(store));
    r = gtk_cell_renderer_text_new();
    gtk_cell_layout_pack_start(GTK_CELL_LAYOUT(w), r, TRUE);
    gtk_cell_layout_set_attributes(GTK_CELL_LAYOUT(w), r, "text", 1, NULL);

    /* cleanup */
    g_object_unref(store);
    return w;
}

GtkWidget *my_scrolledwin_new(GtkWidget *child)
{
    GtkWidget *scrolled_win = gtk_scrolled_window_new();
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scrolled_win), GTK_POLICY_AUTOMATIC,
                                   GTK_POLICY_AUTOMATIC);
    gtk_scrolled_window_set_has_frame(GTK_SCROLLED_WINDOW(scrolled_win), TRUE);
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scrolled_win), child);
    return scrolled_win;
}

/* gtk_widget_set_sensitive() was introduced in 2.18, we can have a minimum of
 * 2.16 otherwise. */

void trg_widget_set_visible(GtkWidget *w, gboolean visible)
{
    gtk_widget_set_visible(w, visible);
}

void trg_client_error_dialog(GtkWindow *parent, trg_response *response)
{
    g_autofree char *msg = make_error_message(response->obj, response->status, response->err_msg);
    trg_error_dialog(parent, msg);
}

GtkWidget *trg_error_dialog(GtkWindow *parent, gchar *msg)
{
    GtkWidget *dialog = gtk_message_dialog_new(parent, GTK_DIALOG_MODAL, GTK_MESSAGE_ERROR,
                                               GTK_BUTTONS_OK, "%s", msg);
    gtk_window_set_title(GTK_WINDOW(dialog), _("Error"));
    g_signal_connect(dialog, "response", G_CALLBACK(gtk_window_destroy), NULL);
    gtk_window_present(GTK_WINDOW(dialog));

    /* Doesn't block like gtk_dialog_run() did, callers can connect to
     * "response" to continue after it's dismissed. */
    return dialog;
}

/* Context menus: GTK 4 has no "button-press-event" or "popup-menu" signals,
 * so use a click gesture for the secondary button and a shortcut for the
 * keyboard (Shift+F10 and the Menu key). x and y are -1 when the menu was
 * requested from the keyboard. */

typedef struct {
    TrgContextMenuFunc func;
    gpointer data;
} trg_context_menu_handler;

static void context_menu_pressed_cb(GtkGestureClick *gesture, gint n_press, gdouble x, gdouble y,
                                    trg_context_menu_handler *handler)
{
    GtkWidget *widget = gtk_event_controller_get_widget(GTK_EVENT_CONTROLLER(gesture));

    if (n_press != 1)
        return;

    if (handler->func(widget, x, y, handler->data))
        gtk_gesture_set_state(GTK_GESTURE(gesture), GTK_EVENT_SEQUENCE_CLAIMED);
}

static gboolean context_menu_shortcut_cb(GtkWidget *widget, GVariant *args, gpointer user_data)
{
    trg_context_menu_handler *handler = user_data;

    return handler->func(widget, -1, -1, handler->data);
}

void trg_widget_add_context_menu(GtkWidget *widget, TrgContextMenuFunc func, gpointer data)
{
    trg_context_menu_handler *handler = g_new0(trg_context_menu_handler, 1);
    GtkGesture *gesture = gtk_gesture_click_new();
    GtkEventController *shortcuts = gtk_shortcut_controller_new();

    handler->func = func;
    handler->data = data;
    g_object_set_data_full(G_OBJECT(widget), "trg-context-menu-handler", handler, g_free);

    gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture), GDK_BUTTON_SECONDARY);
    gtk_event_controller_set_propagation_phase(GTK_EVENT_CONTROLLER(gesture), GTK_PHASE_CAPTURE);
    g_signal_connect(gesture, "pressed", G_CALLBACK(context_menu_pressed_cb), handler);
    gtk_widget_add_controller(widget, GTK_EVENT_CONTROLLER(gesture));

    gtk_shortcut_controller_add_shortcut(
        GTK_SHORTCUT_CONTROLLER(shortcuts),
        gtk_shortcut_new(gtk_shortcut_trigger_parse_string("<Shift>F10|Menu"),
                         gtk_callback_action_new(context_menu_shortcut_cb, handler, NULL)));
    /* capture, so a GtkTreeView's own key handling can't swallow it */
    gtk_event_controller_set_propagation_phase(shortcuts, GTK_PHASE_CAPTURE);
    gtk_widget_add_controller(widget, shortcuts);
}

static gboolean popover_unparent_idle(gpointer data)
{
    GtkWidget *popover = GTK_WIDGET(data);

    if (gtk_widget_get_parent(popover))
        gtk_widget_unparent(popover);

    g_object_unref(popover);

    return G_SOURCE_REMOVE;
}

static gboolean popover_present_tick(GtkWidget *popover, GdkFrameClock *clock, gpointer data)
{
    guint *frames = data;

    if (gtk_widget_get_parent(popover) && gtk_widget_get_visible(popover))
        gtk_popover_present(GTK_POPOVER(popover));

    return ++*frames < 3 ? G_SOURCE_CONTINUE : G_SOURCE_REMOVE;
}

static void popover_closed_cb(GtkPopover *popover, gpointer data)
{
    /* Defer, the activated item's action is dispatched after "closed". */
    g_idle_add(popover_unparent_idle, g_object_ref(popover));
}

/* Pops up a context menu for model on parent, at x/y (-1 for the whole
 * widget). actions (if not NULL) are inserted with prefix for the items. */
void trg_popover_menu_popup(GtkWidget *parent, GMenuModel *model, gdouble x, gdouble y,
                            const gchar *prefix, GActionGroup *actions)
{
    /* nested submenus, like GTK 3 menus, rather than sliding ones */
    GtkWidget *popover = gtk_popover_menu_new_from_model_full(model, GTK_POPOVER_MENU_NESTED);
    GtkWidget *anchor = parent;
    GdkRectangle rect;
    gboolean have_rect = FALSE;

    if (actions)
        gtk_widget_insert_action_group(popover, prefix, actions);

    /* A GtkTreeView keeps the CSS nodes of its header buttons under a
     * "header" node, so adding another child to it upsets GTK. Attach the
     * popover to its parent (the scrolled window) instead. */
    if (GTK_IS_TREE_VIEW(parent) && gtk_widget_get_parent(parent)) {
        graphene_rect_t bounds;
        GtkTreePath *path = NULL;

        anchor = gtk_widget_get_parent(parent);

        /* From the keyboard, point at the cursor row if there is one. */
        if (x < 0 || y < 0)
            gtk_tree_view_get_cursor(GTK_TREE_VIEW(parent), &path, NULL);
        if (path) {
            GdkRectangle area;
            gint wx, wy;

            gtk_tree_view_get_background_area(GTK_TREE_VIEW(parent), path, NULL, &area);
            gtk_tree_view_convert_bin_window_to_widget_coords(GTK_TREE_VIEW(parent), area.x,
                                                              area.y + area.height / 2, &wx, &wy);
            x = MAX(wx, 0) + 20;
            y = wy;
            gtk_tree_path_free(path);
        }

        if (x >= 0 && y >= 0) {
            graphene_point_t p;
            if (gtk_widget_compute_point(parent, anchor, &GRAPHENE_POINT_INIT((float)x, (float)y),
                                         &p)) {
                x = p.x;
                y = p.y;
            }
        } else if (gtk_widget_compute_bounds(parent, anchor, &bounds)) {
            rect.x = (gint)bounds.origin.x;
            rect.y = (gint)bounds.origin.y;
            rect.width = (gint)bounds.size.width;
            rect.height = (gint)bounds.size.height;
            have_rect = TRUE;
        }
    }

    gtk_widget_set_parent(popover, anchor);
    gtk_popover_set_has_arrow(GTK_POPOVER(popover), FALSE);
    gtk_widget_set_halign(popover, GTK_ALIGN_START);

    if (x >= 0 && y >= 0) {
        rect.x = (gint)x;
        rect.y = (gint)y;
        rect.width = rect.height = 1;
        have_rect = TRUE;
    }

    if (have_rect)
        gtk_popover_set_pointing_to(GTK_POPOVER(popover), &rect);

    g_signal_connect(popover, "closed", G_CALLBACK(popover_closed_cb), NULL);
    gtk_popover_popup(GTK_POPOVER(popover));

    /* Widgets like GtkTreeView don't present popovers they don't know about
     * in their size_allocate, so the menu would keep the size it had before
     * its items were fully set up (eg. accelerator labels). Re-present it
     * for the first few frames. */
    gtk_widget_add_tick_callback(popover, popover_present_tick, g_new0(guint, 1), g_free);
}

gchar *make_error_message(JsonObject *response, gint status, gchar *err_msg)
{
    switch (status) {

    case FAIL_HTTP_UNSUCCESSFUL:
        return g_strdup(err_msg ? err_msg : "Unknown HTTP failure.");

    case FAIL_JSON_DECODE:
        return g_strdup(err_msg ? err_msg : "Unknown JSON decoding error.");

    case FAIL_RESULT_UNSUCCESSFUL: {
        const gchar *resultStr = json_object_get_string_member(response, "result");
        return g_strdup(resultStr ? resultStr : "Server responded, but with no result.");
    }

    case FAIL_NO_SESSION_ID:
        return g_strdup("No \"" TRANSMISSION_SESSION_ID_HEADER
                        "\" header sent, is this the right host?");

    default:
        return g_strdup_printf(_("Request failed with HTTP code: %d"), status);
    }
}

/* Formatters and Transmission basic utility functions.. */

char *tr_strlpercent(char *buf, double x, size_t buflen)
{
    int precision;
    if (x < 10.0)
        precision = 2;
    else if (x < 100.0)
        precision = 1;
    else
        precision = 0;

    g_snprintf(buf, buflen, "%.*f%%", precision, tr_truncd(x, precision));
    return buf;
}

double tr_truncd(double x, int decimal_places)
{
    const int i = (int)pow(10, decimal_places);
    double x2 = (int)(x * i);
    return x2 / i;
}

char *tr_strratio(char *buf, size_t buflen, double ratio, const char *infinity)
{
    if (isnan(ratio))
        g_strlcpy(buf, _("None"), buflen);
    else if (isinf(ratio))
        g_strlcpy(buf, infinity, buflen);
    else if (ratio < 10.0)
        g_snprintf(buf, buflen, "%.2f", tr_truncd(ratio, 2));
    else if (ratio < 100.0)
        g_snprintf(buf, buflen, "%.1f", tr_truncd(ratio, 1));
    else
        g_snprintf(buf, buflen, "%'.0f", ratio);
    return buf;
}

char *tr_strlratio(char *buf, double ratio, size_t buflen)
{
    return tr_strratio(buf, buflen, ratio, "\xE2\x88\x9E");
}

char *tr_strltime_short(char *buf, long seconds, size_t buflen)
{
    int hours, minutes;

    if (seconds < 0)
        seconds = 0;

    hours = seconds / 3600;
    minutes = (seconds % 3600) / 60;
    seconds = (seconds % 3600) % 60;

    g_snprintf(buf, buflen, "%02d:%02d:%02ld", hours, minutes, seconds);

    return buf;
}

char *tr_strltime_long(char *buf, long seconds, size_t buflen)
{
    int days, hours, minutes;
    char d[128], h[128], m[128], s[128];

    if (seconds < 0)
        seconds = 0;

    days = seconds / 86400;
    hours = (seconds % 86400) / 3600;
    minutes = (seconds % 3600) / 60;
    seconds = (seconds % 3600) % 60;

    g_snprintf(d, sizeof(d), ngettext("%d day", "%d days", days), days);
    g_snprintf(h, sizeof(h), ngettext("%d hour", "%d hours", hours), hours);
    g_snprintf(m, sizeof(m), ngettext("%d minute", "%d minutes", minutes), minutes);
    g_snprintf(s, sizeof(s), ngettext("%ld second", "%ld seconds", seconds), seconds);

    if (days) {
        if (days >= 4 || !hours) {
            g_strlcpy(buf, d, buflen);
        } else {
            g_snprintf(buf, buflen, "%s, %s", d, h);
        }
    } else if (hours) {
        if (hours >= 4 || !minutes) {
            g_strlcpy(buf, h, buflen);
        } else {
            g_snprintf(buf, buflen, "%s, %s", h, m);
        }
    } else if (minutes) {
        if (minutes >= 4 || !seconds) {
            g_strlcpy(buf, m, buflen);
        } else {
            g_snprintf(buf, buflen, "%s, %s", m, s);
        }
    } else {
        g_strlcpy(buf, s, buflen);
    }

    return buf;
}

char *gtr_localtime(time_t time)
{
    const struct tm tm = *localtime(&time);
    char buf[256], *eoln;

    g_strlcpy(buf, asctime(&tm), sizeof(buf));
    if ((eoln = strchr(buf, '\n')))
        *eoln = '\0';

    return g_locale_to_utf8(buf, -1, NULL, NULL, NULL);
}

char *gtr_localtime2(char *buf, time_t time, size_t buflen)
{
    char *tmp = gtr_localtime(time);
    g_strlcpy(buf, tmp, buflen);
    g_free(tmp);
    return buf;
}

gchar *epoch_to_string(gint64 epoch)
{
    if (epoch == 0)
        return g_strdup(_("N/A"));
    GDateTime *dt = g_date_time_new_from_unix_local(epoch);
    gchar *timestring = g_date_time_format(dt, "%F %H:%M:%S");
    g_date_time_unref(dt);
    return timestring;
}

/* wrap a link in text with a hyperlink, for use in pango markup.
 * with or without any links - a newly allocated string is returned.
 * Note that a markup-escaped string is always returned. */

gchar *add_links_to_text(const gchar *original)
{
    /* return if original already contains links */
    if (g_regex_match_simple("<a\\s.*>", original, 0, 0)) {
        return g_strdup(original);
    }

    gchar *newText, *url, *link;
    GMatchInfo *match_info;
    GRegex *regex = g_regex_new("(https?://[a-zA-Z0-9_\\-\\./?=&]+)", 0, 0, NULL);

    // extract url and build escaped link
    g_regex_match(regex, original, 0, &match_info);
    url = g_match_info_fetch(match_info, 1);

    if (url) {
        link = g_markup_printf_escaped("<a href='%s'>%s</a>", url, url);
        newText = g_regex_replace(regex, original, -1, 0, link, 0, NULL);
        g_free(url);
        g_free(link);
    } else {
        newText = g_markup_escape_text(original, -1);
    }

    g_regex_unref(regex);
    g_match_info_unref(match_info);
    return newText;
}

char *tr_strlsize(char *buf, guint64 bytes, size_t buflen)
{
    if (!bytes)
        g_strlcpy(buf, Q_("None"), buflen);
    else
        tr_formatter_size_B(buf, bytes, buflen);

    return buf;
}

/* Replacement for GtkContainer:border-width */
void trg_widget_set_margins(GtkWidget *w, gint margin)
{
    gtk_widget_set_margin_start(w, margin);
    gtk_widget_set_margin_end(w, margin);
    gtk_widget_set_margin_top(w, margin);
    gtk_widget_set_margin_bottom(w, margin);
}

/* Remove the mnemonic underscores from a label, for places in GTK 4 that
 * don't support mnemonics. */
gchar *trg_strip_mnemonic(const gchar *label)
{
    GString *str = g_string_new(NULL);
    const gchar *p;

    for (p = label; *p; p++) {
        if (*p == '_') {
            if (p[1] != '_')
                continue;
            p++;
        }
        g_string_append_c(str, *p);
    }

    return g_string_free(str, FALSE);
}

GtkWidget *trg_hbox_new(gboolean homogeneous, gint spacing)
{
    GtkWidget *box;
    box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, spacing);
    gtk_box_set_homogeneous(GTK_BOX(box), homogeneous);
    return box;
}

GtkWidget *trg_vbox_new(gboolean homogeneous, gint spacing)
{
    GtkWidget *box;
    box = gtk_box_new(GTK_ORIENTATION_VERTICAL, spacing);
    gtk_box_set_homogeneous(GTK_BOX(box), homogeneous);
    return box;
}

#ifdef G_OS_WIN32
gchar *trg_win32_support_path(gchar *file)
{
    gchar *moddir = g_win32_get_package_installation_directory_of_module(NULL);
    gchar *path = g_build_filename(moddir, file, NULL);
    g_free(moddir);
    return path;
}
#endif
