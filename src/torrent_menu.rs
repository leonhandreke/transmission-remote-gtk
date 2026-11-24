//! The context menu of the torrent list: the torrent actions (sections of
//! the main window's template), and the speed limits and bandwidth priority
//! of the selected torrents, as radio items.

use gtk::prelude::*;
use gtk::{gio, glib};

use crate::client::Client;
use crate::format;
use crate::i18n::tr;
use crate::main_window::MainWindow;
use crate::protocol as p;
use crate::torrent_view;
use crate::tree_view::TreeView;
use crate::util;

/// Pops up the menu for the selected torrents at x, y (-1 for the whole list)
pub fn popup(win: &MainWindow, tv: &TreeView, x: f64, y: f64, sections: &[gio::MenuModel]) {
    let ids = torrent_view::selected_ids(tv);
    // The radio items show the values of the first selected torrent
    let client = win.client();
    let current = client.torrent_data(win.selected_torrent_id()).map(|(t, _)| t);
    let current = current.as_deref();
    let actions = gio::SimpleActionGroup::new();

    let menu = gio::Menu::new();
    for section in sections {
        menu.append_section(None, section);
    }
    let section = gio::Menu::new();
    // the current limit (-1 for none), and the keys of the torrent_set
    let limit = |name, title: &str, limit: Option<(bool, i64)>, enabled_key, speed_key| {
        let limit = limit.filter(|(limited, _)| *limited).map_or(-1, |(_, l)| l as i32);
        torrent_set_action(&client, &actions, &ids, name, limit, move |req, v| {
            if v >= 0 {
                req.set(speed_key, v);
            }
            req.set(enabled_key, v >= 0);
        });
        limit_menu(name, title, limit)
    };
    section.append_item(&limit(
        "down-limit",
        &tr("Down Limit"),
        current.map(|t| (t.download_limited, t.download_limit)),
        p::FIELD_DOWNLOAD_LIMITED,
        p::FIELD_DOWNLOAD_LIMIT,
    ));
    section.append_item(&limit(
        "up-limit",
        &tr("Up Limit"),
        current.map(|t| (t.upload_limited, t.upload_limit)),
        p::FIELD_UPLOAD_LIMITED,
        p::FIELD_UPLOAD_LIMIT,
    ));
    let priority = current.map_or(p::TR_PRI_UNSET, |t| t.bandwidth_priority);
    torrent_set_action(&client, &actions, &ids, "priority", priority as i32, |req, v| {
        req.set(p::FIELD_BANDWIDTH_PRIORITY, v);
    });
    section.append_item(&priority_menu());
    menu.append_section(None, &section);

    util::popover_menu_popup(tv, &menu, x, y, "torrent", Some(&actions));
}

/// A radio action ("torrent.<name>") setting a value of the torrents ids
/// with a torrent_set, its state being the current value
fn torrent_set_action(
    client: &Client,
    actions: &gio::SimpleActionGroup,
    ids: &[i64],
    name: &str,
    state: i32,
    set: impl Fn(&mut p::Request, i32) + 'static,
) {
    let action =
        gio::SimpleAction::new_stateful(name, Some(glib::VariantTy::INT32), &state.to_variant());
    let client = client.clone();
    let ids = ids.to_vec();
    action.connect_change_state(move |action, value| {
        let Some(v) = value.and_then(|v| v.get::<i32>()) else { return };
        action.set_state(&v.to_variant());
        let mut req = p::torrent_set(&ids);
        set(&mut req, v);
        client.dispatch_interactive(req);
    });
    actions.add_action(&action);
}

fn radio_item(label: &str, action: &str, value: i32) -> gio::MenuItem {
    let item = gio::MenuItem::new(Some(label), None);
    item.set_action_and_target_value(Some(action), Some(&value.to_variant()));
    item
}

fn submenu(title: &str, menu: &gio::Menu) -> gio::MenuItem {
    let item = gio::MenuItem::new_submenu(Some(title), menu);
    item.set_icon(&gio::ThemedIcon::new("network-workgroup"));
    item
}

fn priority_menu() -> gio::MenuItem {
    let menu = gio::Menu::new();
    for (label, value) in
        [(tr("High"), p::TR_PRI_HIGH), (tr("Normal"), p::TR_PRI_NORMAL), (tr("Low"), p::TR_PRI_LOW)]
    {
        menu.append_item(&radio_item(&label, "torrent.priority", value as i32));
    }
    submenu(&tr("Priority"), &menu)
}

/// No limit (-1), and the speeds 5 * 2^n KiB/s, and the current limit if it's
/// another one
fn limit_menu(name: &str, title: &str, limit: i32) -> gio::MenuItem {
    let action = format!("torrent.{name}");
    let speed = |kbps: i32| radio_item(&format::speed_kbps(kbps as f64), &action, kbps);

    let section = gio::Menu::new();
    section.append_item(&radio_item(&tr("No Limit"), &action, -1));
    let presets: Vec<i32> = (0..16).map(|i| 5 << i).collect();
    if limit >= 0 && !presets.contains(&limit) {
        // GTK 3 showed this checked but insensitive. A disabled action
        // doesn't show its state in a GTK 4 menu, so this is a normal
        // (checked) radio item, choosing it sets the same limit again.
        section.append_item(&speed(limit));
    }
    let menu = gio::Menu::new();
    menu.append_section(None, &section);

    let section = gio::Menu::new();
    for v in presets {
        section.append_item(&speed(v));
    }
    menu.append_section(None, &section);

    submenu(title, &menu)
}
