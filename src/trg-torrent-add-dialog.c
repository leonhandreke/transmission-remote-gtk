/*
 * transmission-remote-gtk - A GTK RPC client to Transmission
 * Copyright (C) 2011  Alan Fitton

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

/* Most of the UI code was taken from open-dialog.c and files-list.c
 * in Transmission, adapted to fit in with different torrent file parser
 * and JSON dispatch.
 */

#include "config.h"

#include <glib/gi18n.h>
#include <glib/gprintf.h>
#include <gtk/gtk.h>
#include <json-glib/json-glib.h>

#include "hig.h"
#include "json.h"
#include "protocol-constants.h"
#include "requests.h"
#include "torrent.h"
#include "trg-client.h"
#include "trg-destination-combo.h"
#include "trg-file-parser.h"
#include "trg-files-model.h"
#include "trg-files-tree-view.h"
#include "trg-main-window.h"
#include "trg-prefs.h"
#include "trg-torrent-add-dialog.h"
#include "upload.h"
#include "util.h"

enum {
    PROP_0,
    PROP_FILENAME,
    PROP_PARENT,
    PROP_CLIENT,
    PROP_UPLOAD
};

struct _TrgTorrentAddDialog {
    TrgDialog parent;

    TrgClient *client;
    TrgMainWindow *parent_win;
    GSList *filenames;
    trg_upload *upload;
    GtkWidget *source_chooser;
    GtkWidget *dest_combo;
    GtkWidget *priority_combo;
    GtkWidget *file_list;
    TrgFilesModel *files_model;
    GtkWidget *paused_check;
    GtkWidget *delete_check;
};

G_DEFINE_TYPE(TrgTorrentAddDialog, trg_torrent_add_dialog, TRG_TYPE_DIALOG)

#define MAGNET_MAX_LINK_WIDTH 75

static void trg_torrent_add_dialog_set_property(GObject *object, guint prop_id, const GValue *value,
                                                GParamSpec *pspec G_GNUC_UNUSED)
{
    TrgTorrentAddDialog *self = TRG_TORRENT_ADD_DIALOG(object);
    switch (prop_id) {
    case PROP_FILENAME:
        self->filenames = g_value_get_pointer(value);
        break;
    case PROP_PARENT:
        self->parent_win = g_value_get_object(value);
        break;
    case PROP_UPLOAD:
        self->upload = g_value_get_pointer(value);
        break;
    case PROP_CLIENT:
        self->client = g_value_get_pointer(value);
        break;
    default:
        G_OBJECT_WARN_INVALID_PROPERTY_ID(object, prop_id, pspec);
        break;
    }
}

static void trg_torrent_add_dialog_get_property(GObject *object, guint prop_id, GValue *value,
                                                GParamSpec *pspec G_GNUC_UNUSED)
{
    TrgTorrentAddDialog *self = TRG_TORRENT_ADD_DIALOG(object);
    switch (prop_id) {
    case PROP_FILENAME:
        g_value_set_pointer(value, self->filenames);
        break;
    case PROP_PARENT:
        g_value_set_object(value, self->parent_win);
        break;
    default:
        G_OBJECT_WARN_INVALID_PROPERTY_ID(object, prop_id, pspec);
        break;
    }
}

/* The wanted state and priority of each file of the torrent */
static void trg_torrent_add_dialog_get_file_prefs(TrgTorrentAddDialog *d, trg_upload *upload)
{
    guint i, n = trg_files_model_get_n_files(d->files_model);

    upload->n_files = n;
    upload->file_priorities = g_new0(gint, n);
    upload->file_wanted = g_new0(gint, n);

    for (i = 0; i < n; i++) {
        TrgFileItem *item = trg_files_model_get_file(d->files_model, i);

        if (!item)
            continue;

        upload->file_wanted[i] = trg_item_get_int(item, FILESCOL_WANTED);
        upload->file_priorities[i] = trg_item_get_int(item, FILESCOL_PRIORITY);
    }
}

static void trg_torrent_add_response_cb(TrgDialog *dlg, gint res_id, gpointer data)
{
    TrgTorrentAddDialog *d = TRG_TORRENT_ADD_DIALOG(dlg);
    guint flags = 0x00;
    if (gtk_check_button_get_active(GTK_CHECK_BUTTON(d->paused_check)))
        flags |= TORRENT_ADD_FLAG_PAUSED;
    if (gtk_check_button_get_active(GTK_CHECK_BUTTON(d->delete_check)))
        flags |= TORRENT_ADD_FLAG_DELETE;

    if (res_id == GTK_RESPONSE_ACCEPT) {
        gint priority = (gint)gtk_drop_down_get_selected(GTK_DROP_DOWN(d->priority_combo)) - 1;
        gchar *dir = trg_destination_combo_get_dir(TRG_DESTINATION_COMBO(d->dest_combo));
        trg_upload *upload;

        if (d->upload) {
            upload = d->upload;
        } else {
            upload = g_new0(trg_upload, 1);
            upload->list = d->filenames;
        }

        upload->main_window = d->parent_win;
        upload->client = d->client;
        upload->dir = dir;
        upload->priority = priority;
        upload->flags = flags;
        upload->extra_args = TRUE;

        trg_torrent_add_dialog_get_file_prefs(d, upload);

        trg_do_upload(upload);

        trg_destination_combo_save_selection(TRG_DESTINATION_COMBO(d->dest_combo));
    } else {
        g_str_slist_free(d->filenames);
    }

    gtk_window_destroy(GTK_WINDOW(dlg));
}

static void gtr_dialog_set_content(TrgDialog *dialog, GtkWidget *content)
{
    GtkWidget *vbox = trg_dialog_get_content_area(dialog);
    gtk_widget_set_vexpand(content, TRUE);
    gtk_box_append(GTK_BOX(vbox), content);
}

static GtkWidget *gtr_priority_combo_new(void)
{
    /* the selected position - 1 is the priority (TR_PRI_LOW is -1) */
    return gtr_drop_down_new(_("Low"), _("Normal"), _("High"), NULL);
}

static void addTorrentFilters(GtkFileDialog *dialog)
{
    GListStore *filters = g_list_store_new(GTK_TYPE_FILE_FILTER);
    GtkFileFilter *filter;

    filter = gtk_file_filter_new();
    gtk_file_filter_set_name(filter, _("Torrent files"));
    gtk_file_filter_add_pattern(filter, "*.torrent");
    g_list_store_append(filters, filter);
    gtk_file_dialog_set_default_filter(dialog, filter);
    g_object_unref(filter);

    filter = gtk_file_filter_new();
    gtk_file_filter_set_name(filter, _("All files"));
    gtk_file_filter_add_pattern(filter, "*");
    g_list_store_append(filters, filter);
    g_object_unref(filter);

    gtk_file_dialog_set_filters(dialog, G_LIST_MODEL(filters));
    g_object_unref(filters);
}

static void torrent_not_found_error(GtkWindow *parent, gchar *file)
{
    gchar *msg = g_strdup_printf(_("Unable to open torrent file: %s"), file);

    trg_error_dialog(parent, msg);
    g_free(msg);
}

static void trg_torrent_add_dialog_set_filenames(TrgTorrentAddDialog *d, GSList *filenames)
{
    GtkButton *chooser = GTK_BUTTON(d->source_chooser);
    gint nfiles = filenames ? g_slist_length(filenames) : 0;

    trg_files_model_clear(d->files_model);

    if (d->upload) {
        trg_upload_free(d->upload);
        d->upload = NULL;
    }

    if (nfiles == 1) {
        gchar *file_name = (gchar *)filenames->data;
        if (is_url(file_name) || is_magnet(file_name)) {
            if (strlen(file_name) > MAGNET_MAX_LINK_WIDTH) {
                gchar *file_name_trunc = g_strndup(file_name, MAGNET_MAX_LINK_WIDTH);
                gchar *file_name_trunc_fmt = g_strdup_printf("%s ...", file_name_trunc);
                gtk_button_set_label(chooser, file_name_trunc_fmt);
                g_free(file_name_trunc);
                g_free(file_name_trunc_fmt);
            } else {
                gtk_button_set_label(chooser, file_name);
            }

            gtk_widget_set_sensitive(d->file_list, FALSE);
            gtk_widget_set_sensitive(d->delete_check, FALSE);
        } else {
            gchar *file_name_base;
            trg_torrent_file *tor_data = NULL;

            file_name_base = g_path_get_basename(file_name);

            if (file_name_base) {
                gtk_button_set_label(chooser, file_name_base);
            } else {
                gtk_button_set_label(chooser, file_name);
            }

            if (g_file_test(file_name, G_FILE_TEST_IS_REGULAR)) {
                g_autoptr(GError) error = NULL;
                tor_data = trg_parse_torrent_file(file_name, &error);
                if (!tor_data) {
                    if (error) {
                        trg_error_dialog(GTK_WINDOW(d->parent_win), error->message);
                        g_str_slist_free(filenames);
                        d->filenames = NULL;
                        return;
                    }
                } else {
                    trg_files_model_set_tree(d->files_model, tor_data->top_node);
                    trg_torrent_file_free(tor_data);
                }
            } else {
                torrent_not_found_error(GTK_WINDOW(d->parent_win), file_name);
            }

            gtk_widget_set_sensitive(d->file_list, tor_data != NULL);
        }
    } else {
        gtk_widget_set_sensitive(d->file_list, FALSE);
        if (nfiles < 1) {
            gtk_button_set_label(chooser, _("(None)"));
        } else {
            gtk_button_set_label(chooser, _("(Multiple)"));
        }
    }

    d->filenames = filenames;
}

/* Remember the folder of the chosen files for the next time. */
static void trg_torrent_add_dialog_generic_save_dir(GListModel *files, TrgPrefs *prefs)
{
    g_autoptr(GFile) first = g_list_model_get_item(files, 0);
    g_autoptr(GFile) parent = first ? g_file_get_parent(first) : NULL;
    gchar *cwd = parent ? g_file_get_path(parent) : NULL;

    if (cwd) {
        trg_prefs_set_string(prefs, TRG_PREFS_KEY_LAST_TORRENT_DIR, cwd, TRG_PREFS_GLOBAL);
        g_free(cwd);
    }
}

static GSList *trg_torrent_add_dialog_get_filenames(GListModel *files)
{
    GSList *list = NULL;
    guint i;

    for (i = 0; i < g_list_model_get_n_items(files); i++) {
        g_autoptr(GFile) file = g_list_model_get_item(files, i);
        gchar *path = g_file_get_path(file);

        if (path)
            list = g_slist_append(list, path);
    }

    return list;
}

static GtkFileDialog *trg_torrent_add_dialog_generic(TrgPrefs *prefs)
{
    GtkFileDialog *dialog = gtk_file_dialog_new();
    gchar *dir = trg_prefs_get_string(prefs, TRG_PREFS_KEY_LAST_TORRENT_DIR, TRG_PREFS_GLOBAL);

    gtk_file_dialog_set_title(dialog, _("Add a Torrent"));
    gtk_file_dialog_set_accept_label(dialog, _("_Add"));
    gtk_file_dialog_set_modal(dialog, TRUE);

    if (dir) {
        g_autoptr(GFile) folder = g_file_new_for_path(dir);
        gtk_file_dialog_set_initial_folder(dialog, folder);
        g_free(dir);
    }

    addTorrentFilters(dialog);

    return dialog;
}

static void trg_torrent_add_dialog_source_response_cb(GObject *source, GAsyncResult *result,
                                                      gpointer data)
{
    TrgTorrentAddDialog *d = TRG_TORRENT_ADD_DIALOG(data);
    g_autoptr(GListModel) files
        = gtk_file_dialog_open_multiple_finish(GTK_FILE_DIALOG(source), result, NULL);

    if (files && g_list_model_get_n_items(files) > 0) {
        if (d->filenames)
            g_str_slist_free(d->filenames);

        d->filenames = trg_torrent_add_dialog_get_filenames(files);
        trg_torrent_add_dialog_generic_save_dir(files, trg_client_get_prefs(d->client));
        trg_torrent_add_dialog_set_filenames(d, d->filenames);
    }

    g_object_unref(d);
}

static void trg_torrent_add_dialog_source_click_cb(GtkWidget *w, gpointer data)
{
    TrgTorrentAddDialog *d = TRG_TORRENT_ADD_DIALOG(data);
    GtkFileDialog *dialog = trg_torrent_add_dialog_generic(trg_client_get_prefs(d->client));

    gtk_file_dialog_open_multiple(dialog, GTK_WINDOW(data), NULL,
                                  trg_torrent_add_dialog_source_response_cb, g_object_ref(d));
    g_object_unref(dialog);
}

/* The choices of "Apply to all", in the order of the drop down */
static const struct {
    const gchar *label;
    gint column;
    gint value;
} apply_all_choices[] = {
    { N_("High Priority"), FILESCOL_PRIORITY, TR_PRI_HIGH },
    { N_("Normal Priority"), FILESCOL_PRIORITY, TR_PRI_NORMAL },
    { N_("Low Priority"), FILESCOL_PRIORITY, TR_PRI_LOW },
    { N_("Download"), FILESCOL_WANTED, TRUE },
    { N_("Skip"), FILESCOL_WANTED, FALSE },
};

static void trg_torrent_add_dialog_apply_all_cb(GSimpleAction *action, GVariant *parameter,
                                                gpointer data)
{
    TrgTorrentAddDialog *d = TRG_TORRENT_ADD_DIALOG(data);
    GListModel *top = G_LIST_MODEL(d->files_model);
    gint choice = g_variant_get_int32(parameter);
    guint i;

    if (choice < 0 || choice >= (gint)G_N_ELEMENTS(apply_all_choices))
        return;

    for (i = 0; i < g_list_model_get_n_items(top); i++) {
        g_autoptr(TrgFileItem) item = g_list_model_get_item(top, i);

        if (apply_all_choices[choice].column == FILESCOL_WANTED)
            trg_file_item_set_wanted(item, apply_all_choices[choice].value);
        else
            trg_file_item_set_priority(item, apply_all_choices[choice].value);
    }

    trg_tree_view_resort(TRG_TREE_VIEW(d->file_list));
}

/* "Apply to all" is an action rather than a setting, so a menu button */
static GtkWidget *trg_torrent_add_dialog_apply_all_new(TrgTorrentAddDialog *dialog)
{
    static const GActionEntry entries[] = {
        { "apply-all", trg_torrent_add_dialog_apply_all_cb, "i", NULL, NULL, { 0 } },
    };
    GSimpleActionGroup *actions = g_simple_action_group_new();
    GMenu *menu = g_menu_new();
    GMenu *section = NULL;
    GtkWidget *button = gtk_menu_button_new();
    gint i;

    g_action_map_add_action_entries(G_ACTION_MAP(actions), entries, G_N_ELEMENTS(entries), dialog);
    gtk_widget_insert_action_group(GTK_WIDGET(dialog), "add", G_ACTION_GROUP(actions));
    g_object_unref(actions);

    for (i = 0; i < (gint)G_N_ELEMENTS(apply_all_choices); i++) {
        GMenuItem *item = g_menu_item_new(_(apply_all_choices[i].label), NULL);

        /* the priorities, then download/skip */
        if (!section || (i > 0 && apply_all_choices[i].column != apply_all_choices[i - 1].column)) {
            if (section) {
                g_menu_append_section(menu, NULL, G_MENU_MODEL(section));
                g_object_unref(section);
            }
            section = g_menu_new();
        }

        g_menu_item_set_action_and_target_value(item, "add.apply-all", g_variant_new_int32(i));
        g_menu_append_item(section, item);
        g_object_unref(item);
    }

    g_menu_append_section(menu, NULL, G_MENU_MODEL(section));
    g_object_unref(section);

    gtk_menu_button_set_menu_model(GTK_MENU_BUTTON(button), G_MENU_MODEL(menu));
    gtk_menu_button_set_always_show_arrow(GTK_MENU_BUTTON(button), TRUE);
    gtk_widget_set_halign(button, GTK_ALIGN_START);
    g_object_unref(menu);

    /* nothing to apply to without a file list */
    g_object_bind_property(dialog->file_list, "sensitive", button, "sensitive",
                           G_BINDING_SYNC_CREATE);

    return button;
}

static GObject *trg_torrent_add_dialog_constructor(GType type, guint n_construct_properties,
                                                   GObjectConstructParam *construct_params)
{
    GObject *obj = G_OBJECT_CLASS(trg_torrent_add_dialog_parent_class)
                       ->constructor(type, n_construct_properties, construct_params);
    TrgTorrentAddDialog *self = TRG_TORRENT_ADD_DIALOG(obj);
    TrgPrefs *prefs = trg_client_get_prefs(self->client);
    GtkWidget *t, *applyall_combo;
    guint row = 0;

    /* window */
    gtk_window_set_title(GTK_WINDOW(obj), _("Add Torrent"));
    gtk_window_set_transient_for(GTK_WINDOW(obj), GTK_WINDOW(self->parent_win));
    gtk_window_set_destroy_with_parent(GTK_WINDOW(obj), TRUE);

    /* buttons */
    trg_dialog_add_button(TRG_DIALOG(obj), _("_Cancel"), GTK_RESPONSE_CANCEL);
    trg_dialog_add_button(TRG_DIALOG(obj), _("_Open"), GTK_RESPONSE_ACCEPT);
    trg_dialog_set_default_response(TRG_DIALOG(obj), GTK_RESPONSE_ACCEPT);
    gtk_widget_grab_focus(trg_dialog_get_widget_for_response(TRG_DIALOG(obj), GTK_RESPONSE_ACCEPT));

    /* workspace */
    t = hig_workarea_create();
    // gtk_container_set_border_width(GTK_CONTAINER(t), GUI_PAD_BIG);

    self->files_model = trg_files_model_new();
    self->file_list = GTK_WIDGET(trg_files_tree_view_new_local(self->files_model, NULL, NULL));
    gtk_widget_set_sensitive(self->file_list, FALSE);

    self->paused_check = gtk_check_button_new_with_mnemonic(_("Start _paused"));
    gtk_check_button_set_active(
        GTK_CHECK_BUTTON(self->paused_check),
        trg_prefs_get_bool(prefs, TRG_PREFS_KEY_START_PAUSED, TRG_PREFS_GLOBAL));

    self->delete_check
        = gtk_check_button_new_with_mnemonic(_("Delete local .torrent file after adding"));
    gtk_check_button_set_active(
        GTK_CHECK_BUTTON(self->delete_check),
        trg_prefs_get_bool(prefs, TRG_PREFS_KEY_DELETE_LOCAL_TORRENT, TRG_PREFS_GLOBAL));

    self->priority_combo = gtr_priority_combo_new();
    gtk_drop_down_set_selected(GTK_DROP_DOWN(self->priority_combo), 1);

    self->source_chooser = gtk_button_new();
    hig_workarea_add_row(t, &row, _("_Torrent file:"), self->source_chooser, NULL);

    if (self->filenames)
        trg_torrent_add_dialog_set_filenames(TRG_TORRENT_ADD_DIALOG(obj), self->filenames);

    g_signal_connect(self->source_chooser, "clicked",
                     G_CALLBACK(trg_torrent_add_dialog_source_click_cb), obj);

    self->dest_combo = trg_destination_combo_new(self->client, TRG_PREFS_KEY_LAST_ADD_DESTINATION);

    hig_workarea_add_row(t, &row, _("_Destination folder:"), self->dest_combo, NULL);

    gtk_widget_set_size_request(self->file_list, 466u, 300u);

    hig_workarea_add_wide_tall_control(t, &row, self->file_list);

    applyall_combo = trg_torrent_add_dialog_apply_all_new(TRG_TORRENT_ADD_DIALOG(obj));

    hig_workarea_add_row(t, &row, _("Apply to all:"), applyall_combo, NULL);

    hig_workarea_add_row(t, &row, _("Torrent _priority:"), self->priority_combo, NULL);

    hig_workarea_add_wide_control(t, &row, self->paused_check);
    hig_workarea_add_wide_control(t, &row, self->delete_check);

    gtr_dialog_set_content(TRG_DIALOG(obj), t);

    g_signal_connect(G_OBJECT(obj), "response", G_CALLBACK(trg_torrent_add_response_cb),
                     self->parent_win);

    return obj;
}

static void trg_torrent_add_dialog_dispose(GObject *object)
{
    g_clear_object(&TRG_TORRENT_ADD_DIALOG(object)->files_model);

    G_OBJECT_CLASS(trg_torrent_add_dialog_parent_class)->dispose(object);
}

static void trg_torrent_add_dialog_class_init(TrgTorrentAddDialogClass *klass)
{
    GObjectClass *object_class = G_OBJECT_CLASS(klass);

    object_class->set_property = trg_torrent_add_dialog_set_property;
    object_class->get_property = trg_torrent_add_dialog_get_property;
    object_class->constructor = trg_torrent_add_dialog_constructor;
    object_class->dispose = trg_torrent_add_dialog_dispose;

    g_object_class_install_property(
        object_class, PROP_FILENAME,
        g_param_spec_pointer("filenames", "filenames", "filenames",
                             G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                 | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    g_object_class_install_property(
        object_class, PROP_UPLOAD,
        g_param_spec_pointer("upload", "upload", "upload",
                             G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                 | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    g_object_class_install_property(
        object_class, PROP_CLIENT,
        g_param_spec_pointer("client", "client", "client",
                             G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                 | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));

    g_object_class_install_property(
        object_class, PROP_PARENT,
        g_param_spec_object("parent", "parent", "parent", TRG_TYPE_MAIN_WINDOW,
                            G_PARAM_READWRITE | G_PARAM_CONSTRUCT_ONLY | G_PARAM_STATIC_NAME
                                | G_PARAM_STATIC_NICK | G_PARAM_STATIC_BLURB));
}

static void trg_torrent_add_dialog_init(TrgTorrentAddDialog *self)
{
}

TrgTorrentAddDialog *trg_torrent_add_dialog_new_from_filenames(TrgMainWindow *parent,
                                                               TrgClient *client, GSList *filenames)
{
    return g_object_new(TRG_TYPE_TORRENT_ADD_DIALOG, "filenames", filenames, "parent", parent,
                        "client", client, NULL);
}

typedef struct {
    TrgMainWindow *win;
    TrgClient *client;
} trg_add_chooser_data;

static void trg_torrent_add_dialog_chooser_response_cb(GObject *source, GAsyncResult *result,
                                                       gpointer user_data)
{
    trg_add_chooser_data *data = user_data;
    TrgMainWindow *win = data->win;
    TrgClient *client = data->client;
    TrgPrefs *prefs = trg_client_get_prefs(client);
    g_autoptr(GListModel) files
        = gtk_file_dialog_open_multiple_finish(GTK_FILE_DIALOG(source), result, NULL);

    if (files && g_list_model_get_n_items(files) > 0) {
        /* A GtkFileDialog can't have extra options, so whether the options
         * dialog is shown follows the preference ("Options dialog on add"). */
        gboolean showOptions
            = trg_prefs_get_bool(prefs, TRG_PREFS_KEY_ADD_OPTIONS_DIALOG, TRG_PREFS_GLOBAL);
        GSList *l = trg_torrent_add_dialog_get_filenames(files);

        trg_torrent_add_dialog_generic_save_dir(files, prefs);

        if (showOptions) {
            TrgTorrentAddDialog *dialog = trg_torrent_add_dialog_new_from_filenames(win, client, l);
            gtk_window_present(GTK_WINDOW(dialog));
        } else {
            trg_upload *upload = g_new0(trg_upload, 1);

            upload->list = l;
            upload->main_window = win;
            upload->client = client;
            upload->extra_args = FALSE;
            upload->flags = trg_prefs_get_add_flags(prefs);

            trg_do_upload(upload);
        }
    }

    g_free(data);
}

void trg_torrent_add_dialog(TrgMainWindow *win, TrgClient *client)
{
    GtkFileDialog *dialog = trg_torrent_add_dialog_generic(trg_client_get_prefs(client));
    trg_add_chooser_data *data = g_new0(trg_add_chooser_data, 1);

    data->win = win;
    data->client = client;
    gtk_file_dialog_open_multiple(dialog, GTK_WINDOW(win), NULL,
                                  trg_torrent_add_dialog_chooser_response_cb, data);
    g_object_unref(dialog);
}
