/*
 * icons.[ch] written by Paolo Bacchilega, who writes:
 * "There is no problem for me, you can license my code
 * under whatever licence you wish :)"
 *
 * $Id: icons.c 12639 2011-08-07 18:41:13Z jordan $
 */

#include "config.h"

#include "icons.h"
#include <gio/gio.h>
#include <glib.h>
#include <string.h> /* strcmp */

static const char *get_static_string(const char *s)
{
    static GStringChunk *static_strings = NULL;

    if (s == NULL)
        return NULL;

    if (static_strings == NULL)
        static_strings = g_string_chunk_new(1024);

    return g_string_chunk_insert_const(static_strings, s);
}

/* In GTK 4 icons are looked up and scaled by the renderer (GtkCellRendererPixbuf
 * "gicon" + "icon-size"), so only the GIcon for each MIME type is cached here. */
static GHashTable *icon_cache = NULL;

GIcon *gtr_get_mime_type_icon(const char *mime_type)
{
    const char *key = get_static_string(mime_type);
    GIcon *icon;

    if (icon_cache == NULL)
        icon_cache = g_hash_table_new_full(g_str_hash, g_str_equal, NULL, g_object_unref);

    icon = g_hash_table_lookup(icon_cache, key);
    if (icon == NULL) {
        icon = g_content_type_get_icon(mime_type);

        /* same fallback the theme lookup used in GTK 3 */
        if (G_IS_THEMED_ICON(icon))
            g_themed_icon_append_name(G_THEMED_ICON(icon), "text-x-generic");

        g_hash_table_insert(icon_cache, (gpointer)key, icon);
    }

    return g_object_ref(icon);
}

const char *gtr_get_mime_type_from_filename(const char *file G_GNUC_UNUSED)
{
    char *tmp = g_content_type_guess(file, NULL, 0, NULL);
    const char *ret = get_static_string(tmp);
    g_free(tmp);
    return ret;
}
