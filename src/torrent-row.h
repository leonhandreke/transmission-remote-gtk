/*
 * This file Copyright (C) Mnemosyne LLC
 *
 * This file is licensed by the GPL version 2. Works owned by the
 * Transmission project are granted a special exemption to clause 2(b)
 * so that the bulk of its code can remain under the MIT license.
 * This exemption does not extend to derived works not owned by
 * the Transmission project.
 *
 * $Id: torrent-cell-renderer.h 12658 2011-08-09 05:47:24Z jordan $
 */
#pragma once

#include <gtk/gtk.h>

#include "trg-client.h"
#include "trg-torrent-model.h"

#define GTR_UNICODE_UP      "\xE2\x86\x91"
#define GTR_UNICODE_DOWN    "\xE2\x86\x93"
#define DIRECTORY_MIME_TYPE "folder"
#define FILE_MIME_TYPE      "file"
#define UNKNOWN_MIME_TYPE   "unknown"

/* The row of a torrent in the Transmission styles of the torrent list: its
 * icon, name, progress and status, on several lines, or on one when compact. */
#define TORRENT_TYPE_ROW (torrent_row_get_type())
G_DECLARE_FINAL_TYPE(TorrentRow, torrent_row, TORRENT, ROW, GtkWidget)

GtkWidget *torrent_row_new(TrgClient *client, gboolean compact);
void torrent_row_set_item(TorrentRow *row, TrgTorrentItem *item);
