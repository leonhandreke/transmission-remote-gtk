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

#ifndef TRG_PEERS_MODEL_H_
#define TRG_PEERS_MODEL_H_

#include <glib-object.h>
#include <json-glib/json-glib.h>

#include "trg-model.h"
#include "trg-tree-view.h"

G_BEGIN_DECLS

#define TRG_TYPE_PEERS_MODEL trg_peers_model_get_type()
G_DECLARE_FINAL_TYPE(TrgPeersModel, trg_peers_model, TRG, PEERS_MODEL, TrgListModel)

TrgPeersModel *trg_peers_model_new(void);

enum {
    PEERSCOL_ICON,
    PEERSCOL_IP,
    PEERSCOL_HOST,
    PEERSCOL_FLAGS,
    PEERSCOL_PROGRESS,
    PEERSCOL_DOWNSPEED,
    PEERSCOL_UPSPEED,
    PEERSCOL_CLIENT,
    PEERSCOL_UPDATESERIAL,
    PEERSCOL_COLUMNS
};

void trg_peers_model_update(TrgPeersModel *model, TrgTreeView *tv, gint64 updateSerial,
                            JsonObject *t, gint mode);
void trg_peers_model_clear(TrgPeersModel *model);

G_END_DECLS

#endif /* TRG_PEERS_MODEL_H_ */
