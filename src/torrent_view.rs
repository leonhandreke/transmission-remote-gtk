//! The list of torrents: the columns of a TreeView in the classic style, a list
//! of TorrentRows in the Transmission styles.

use std::cell::{Cell, RefCell};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib, pango};

use crate::client::Client;
use crate::format;
use crate::i18n::{ntr, sprintf, tr};
use crate::prefs::{self, Scope};
use crate::protocol::Torrent;
use crate::protocol::{
    TORRENT_FLAG_CHECKING, TORRENT_FLAG_COMPLETE, TORRENT_FLAG_DOWNLOADING,
    TORRENT_FLAG_DOWNLOADING_WAIT, TORRENT_FLAG_PAUSED, TORRENT_FLAG_SEEDING,
    TORRENT_FLAG_SEEDING_WAIT, TORRENT_FLAG_WAITING_CHECK,
};
use crate::torrent_object::{TorrentObject, TorrentValues};
use crate::tree_view::{self, COLUMN_EXTRA, COLUMN_HIDE_FROM_TOP_MENU, ColType, TreeView};

/// Paused or queued
const INACTIVE: u32 =
    TORRENT_FLAG_PAUSED | TORRENT_FLAG_DOWNLOADING_WAIT | TORRENT_FLAG_SEEDING_WAIT;

const UNICODE_UP: &str = "\u{2191}";
const UNICODE_DOWN: &str = "\u{2193}";
const SMALL_SCALE: f64 = 0.9;
const BAR_WIDTH: i32 = 50;
const GUI_PAD_SMALL: i32 = 3;
const GUI_PAD: i32 = 6;

/// The icon of a MIME type ("folder", "file" or "unknown"), with the same
/// fallback as the GTK 3 theme lookup
fn mime_type_icon(mime_type: &str) -> gio::Icon {
    let icon = gio::content_type_get_icon(mime_type);
    if let Some(themed) = icon.downcast_ref::<gio::ThemedIcon>() {
        themed.append_name("text-x-generic");
    }
    icon
}

/// The torrent list view, its columns registered
pub fn torrent_tree_view_new(client: &Client, model: &impl IsA<gio::ListModel>) -> TreeView {
    let prefs = client.prefs();
    let tv = TreeView::new(Some(prefs), "TrgTorrentTreeView");

    // The keys are the model columns the torrents had as Items. Sorting
    // compares the values directly, a few thousand torrents are sorted after
    // each update.
    let reg = |t, key, property, h: &str, id: &str, f| {
        let desc = tv.reg_property(t, key, property, h, id, f);
        let compare = TorrentValues::compare_fn(property).expect("a property of the values");
        desc.set_compare(move |a: &TorrentObject, b| compare(&a.values(), &b.values()));
        desc
    };
    reg(ColType::IconText, 1, "name", &tr("Name"), "name", 0).set_extra("icon");
    reg(ColType::Size, 2, "size-when-done", &tr("Size"), "size", 0);
    reg(ColType::Prog, 3, "percent-done", &tr("Done"), "done", 0);
    reg(ColType::Text, 5, "status", &tr("Status"), "status", 0);
    reg(ColType::NumGtZero, 6, "seeders", &tr("Seeds"), "seeds", 0);
    reg(ColType::NumGtZero, 12, "peers-sending-to-us", &tr("Sending"), "sending", COLUMN_EXTRA);
    reg(ColType::NumGtZero, 7, "leechers", &tr("Leechers"), "leechers", 0);
    reg(ColType::NumGtZero, 8, "downloads", &tr("Downloads"), "downloads", COLUMN_EXTRA);
    reg(
        ColType::NumGtZero,
        10,
        "peers-getting-from-us",
        &tr("Receiving"),
        "connected-leechers",
        COLUMN_EXTRA,
    );
    reg(ColType::NumGtZero, 9, "peers-connected", &tr("Connected"), "connected-peers", 0);
    let hidden = COLUMN_EXTRA | COLUMN_HIDE_FROM_TOP_MENU;
    reg(ColType::NumGtZero, 31, "from-pex", &tr("PEX Peers"), "from-pex", hidden);
    reg(ColType::NumGtZero, 32, "from-dht", &tr("DHT Peers"), "from-dht", hidden);
    reg(ColType::NumGtZero, 33, "from-trackers", &tr("Tracker Peers"), "from-trackers", hidden);
    reg(ColType::NumGtZero, 34, "from-ltep", &tr("LTEP Peers"), "from-ltep", hidden);
    reg(ColType::NumGtZero, 35, "from-resume", &tr("Resumed Peers"), "from-resume", hidden);
    reg(ColType::NumGtZero, 36, "from-incoming", &tr("Incoming Peers"), "from-incoming", hidden);
    reg(ColType::Text, 37, "peer-sources", &tr("Peers T/I/E/H/X/L/R"), "peer-sources", hidden);
    reg(ColType::Speed, 13, "down-speed", &tr("Down Speed"), "down-speed", 0);
    reg(ColType::Speed, 14, "up-speed", &tr("Up Speed"), "up-speed", 0);
    reg(ColType::Eta, 15, "eta", &tr("ETA"), "eta", 0);
    reg(ColType::Size, 16, "uploaded", &tr("Uploaded"), "uploaded", 0);
    reg(ColType::Size, 17, "downloaded", &tr("Downloaded"), "downloaded", 0);
    reg(ColType::Ratio, 21, "ratio", &tr("Ratio"), "ratio", 0);
    reg(ColType::Epoch, 22, "added", &tr("Added"), "added", 0);
    reg(ColType::Text, 38, "tracker-host", &tr("First Tracker"), "first-tracker", COLUMN_EXTRA);
    reg(ColType::Text, 27, "download-dir", &tr("Location"), "download-dir", COLUMN_EXTRA);
    reg(ColType::Text, 23, "id", &tr("ID"), "id", COLUMN_EXTRA);
    reg(ColType::Prio, 29, "bandwidth-priority", &tr("Priority"), "priority", COLUMN_EXTRA);
    reg(
        ColType::NumGtEqZero,
        39,
        "queue-position",
        &tr("Queue Position"),
        "queue-position",
        COLUMN_EXTRA,
    );
    reg(ColType::Epoch, 30, "done-date", &tr("Completed"), "done-date", COLUMN_EXTRA);
    reg(ColType::Epoch, 40, "last-active", &tr("Last Active"), "last-active", COLUMN_EXTRA);

    // No type ahead: typing filters the list instead (see MainWindow)

    tv.set_model(model);
    tv.setup_columns();
    tv.restore_sort();

    set_style(&tv, client, prefs.get_int(prefs::KEY_STYLE, Scope::Global));

    let weak = tv.downgrade();
    let c = client.downgrade();
    prefs.connect_changed(move |prefs, key| {
        if key == Some(prefs::KEY_STYLE)
            && let (Some(tv), Some(client)) = (weak.upgrade(), c.upgrade())
        {
            set_style(&tv, &client, prefs.get_int(prefs::KEY_STYLE, Scope::Global));
        }
    });

    tv
}

/// The IDs of the selected torrents
pub fn selected_ids(tv: &TreeView) -> Vec<i64> {
    tv.selected_items::<TorrentObject>().iter().map(TorrentObject::id).collect()
}

/// The columns, or the rows of the (compact) Transmission style
fn set_style(tv: &TreeView, client: &Client, style: i64) {
    if style == prefs::STYLE_CLASSIC {
        tv.set_list_factory(None);
        return;
    }

    let compact = style == prefs::STYLE_TR_COMPACT;
    let factory = gtk::SignalListItemFactory::new();
    let c = client.downgrade();
    factory.connect_setup(move |_, obj| {
        let (Some(li), Some(client)) = (obj.downcast_ref::<gtk::ListItem>(), c.upgrade()) else {
            return;
        };
        let row = TorrentRow::new(&client, compact);
        tree_view::mark_row_widget(&row, li);
        li.set_child(Some(&row));
    });
    factory.connect_bind(|_, obj| {
        let Some(li) = obj.downcast_ref::<gtk::ListItem>() else { return };
        if let Some(row) = li.child().and_downcast::<TorrentRow>() {
            row.set_torrent(li.item().and_downcast::<TorrentObject>().as_ref());
        }
    });
    factory.connect_unbind(|_, obj| {
        let Some(li) = obj.downcast_ref::<gtk::ListItem>() else { return };
        if let Some(row) = li.child().and_downcast::<TorrentRow>() {
            row.set_torrent(None);
        }
    });
    tv.set_list_factory(Some(factory.upcast_ref()));
}

/* A row of the Transmission styles */

/// The seed ratio goal of the torrent, if it has one
fn seed_ratio(t: &Torrent, client: &Client) -> Option<f64> {
    if t.seed_ratio_mode == 0 {
        client.seed_ratio_limit()
    } else if t.seed_ratio_mode == 1 {
        Some(t.seed_ratio_limit)
    } else {
        None
    }
}

fn progress_string(t: &Torrent, v: &TorrentValues, client: &Client) -> String {
    let have_total = t.have_unchecked + t.have_valid;
    let is_seed = t.have_valid >= t.total_size;
    let seed_ratio = seed_ratio(t, client);
    let size = format::size_or_none;

    let mut s = if v.flags & TORRENT_FLAG_DOWNLOADING != 0 {
        sprintf(
            &tr("%1$s of %2$s (%3$s)"),
            &[&size(have_total), &size(t.size_when_done), &format::percent(v.percent_done)],
        )
    } else if !is_seed {
        match seed_ratio {
            Some(goal) => sprintf(
                &tr("%1$s of %2$s (%3$s), uploaded %4$s (Ratio: %5$s Goal: %6$s)"),
                &[
                    &size(have_total),
                    &size(t.total_size),
                    &format::percent(v.percent_done),
                    &size(t.uploaded_ever),
                    &format::ratio(v.ratio),
                    &format::ratio(goal),
                ],
            ),
            None => sprintf(
                &tr("%1$s of %2$s (%3$s), uploaded %4$s (Ratio: %5$s)"),
                &[
                    &size(have_total),
                    &size(t.total_size),
                    &format::percent(v.percent_done),
                    &size(t.uploaded_ever),
                    &format::ratio(v.ratio),
                ],
            ),
        }
    } else {
        match seed_ratio {
            Some(goal) => sprintf(
                &tr("%1$s, uploaded %2$s (Ratio: %3$s Goal: %4$s)"),
                &[
                    &size(t.total_size),
                    &size(t.uploaded_ever),
                    &format::ratio(v.ratio),
                    &format::ratio(goal),
                ],
            ),
            None => sprintf(
                &tr("%1$s, uploaded %2$s (Ratio: %3$s)"),
                &[&size(t.size_when_done), &size(t.uploaded_ever), &format::ratio(v.ratio)],
            ),
        }
    };

    // add time when downloading
    if v.flags & TORRENT_FLAG_DOWNLOADING != 0
        || (seed_ratio.is_some() && v.flags & TORRENT_FLAG_SEEDING != 0)
    {
        s.push_str(" - ");
        if t.eta < 0 {
            s.push_str(&tr("Remaining time unknown"));
        } else {
            s.push_str(&sprintf(&tr("%s remaining"), &[&format::time_long(t.eta)]));
        }
    }

    s
}

fn short_transfer_string(t: &Torrent) -> String {
    let have_meta = !t.files.is_empty();
    let have_up = have_meta && t.peers_getting_from_us > 0;
    let have_down = have_meta && t.peers_sending_to_us > 0;
    let down = format::speed_kbps((t.rate_download / format::SPEED_K) as f64);
    let up = format::speed_kbps((t.rate_upload / format::SPEED_K) as f64);

    if have_down && have_up {
        sprintf(&tr("%1$s %2$s, %3$s %4$s"), &[&UNICODE_DOWN, &down, &UNICODE_UP, &up])
    } else if have_down {
        sprintf(&tr("%1$s %2$s"), &[&UNICODE_DOWN, &down])
    } else if have_up {
        sprintf(&tr("%1$s %2$s"), &[&UNICODE_UP, &up])
    } else if have_meta {
        tr("Idle")
    } else {
        String::new()
    }
}

fn short_status_string(t: &Torrent, v: &TorrentValues) -> String {
    let flags = v.flags;
    if flags & TORRENT_FLAG_PAUSED != 0 {
        if flags & TORRENT_FLAG_COMPLETE != 0 { tr("Finished") } else { tr("Paused") }
    } else if flags & TORRENT_FLAG_WAITING_CHECK != 0 {
        tr("Queued for verification")
    } else if flags & TORRENT_FLAG_DOWNLOADING_WAIT != 0 {
        tr("Queued for download")
    } else if flags & TORRENT_FLAG_SEEDING_WAIT != 0 {
        tr("Queued for seeding")
    } else if flags & TORRENT_FLAG_CHECKING != 0 {
        sprintf(&tr("Verifying data (%1$s tested)"), &[&format::percent(v.percent_done)])
    } else if flags & (TORRENT_FLAG_DOWNLOADING | TORRENT_FLAG_SEEDING) != 0 {
        let transfer = short_transfer_string(t);
        if flags & TORRENT_FLAG_DOWNLOADING != 0 {
            transfer
        } else {
            // seeding
            format!("{}, {transfer}", sprintf(&tr("Ratio %s"), &[&format::ratio(v.ratio)]))
        }
    } else {
        String::new()
    }
}

fn status_string(t: &Torrent, v: &TorrentValues) -> String {
    let flags = v.flags;
    let mut s = if t.error != 0 {
        match t.error {
            1 => sprintf(&tr("Tracker gave a warning: \"%s\""), &[&t.error_string]),
            2 => sprintf(&tr("Tracker gave an error: \"%s\""), &[&t.error_string]),
            3 => sprintf(&tr("Error: \"%s\""), &[&t.error_string]),
            _ => String::new(),
        }
    } else if flags
        & (TORRENT_FLAG_PAUSED
            | TORRENT_FLAG_WAITING_CHECK
            | TORRENT_FLAG_CHECKING
            | TORRENT_FLAG_DOWNLOADING_WAIT
            | TORRENT_FLAG_SEEDING_WAIT)
        != 0
    {
        short_status_string(t, v)
    } else if flags & TORRENT_FLAG_DOWNLOADING != 0 {
        if !t.files.is_empty() {
            let n = t.webseeds_sending_to_us + t.peers_sending_to_us;
            sprintf(
                &ntr(
                    "Downloading from %1$lu of %2$lu connected peer",
                    "Downloading from %1$lu of %2$lu connected peers",
                    n,
                ),
                &[&n, &(t.webseeds_sending_to_us + t.peers_connected)],
            )
        } else {
            let n = t.peers_connected + t.webseeds_sending_to_us;
            sprintf(
                &ntr(
                    "Downloading metadata from %1$lu peer (%2$s done)",
                    "Downloading metadata from %1$lu peers (%2$s done)",
                    n,
                ),
                &[&n, &format::percent(t.metadata_percent())],
            )
        }
    } else if flags & TORRENT_FLAG_SEEDING != 0 {
        sprintf(
            &ntr(
                "Seeding to %1$lu of %2$lu connected peer",
                "Seeding to %1$lu of %2$lu connected peers",
                t.peers_connected,
            ),
            &[&t.peers_getting_from_us, &t.peers_connected],
        )
    } else {
        String::new()
    };

    // the speeds, unless it's paused, waiting or being checked
    let transfer = short_transfer_string(t);
    if flags & (INACTIVE | TORRENT_FLAG_WAITING_CHECK | TORRENT_FLAG_CHECKING) == 0
        && !transfer.is_empty()
    {
        s.push_str(" - ");
        s.push_str(&transfer);
    }
    s
}

/// Of the seed ratio goal while it's seeding towards one
fn percent_done(t: &Torrent, v: &TorrentValues, client: &Client) -> f64 {
    if v.flags & TORRENT_FLAG_SEEDING != 0 && seed_ratio(t, client).is_some() {
        let downloaded = t.downloaded_ever;
        let baseline = if downloaded != 0 { downloaded } else { t.size_when_done };
        let goal = (baseline as f64 * t.seed_ratio_limit) as i64;
        let uploaded = t.uploaded_ever;
        let bytes_left = if goal > uploaded { goal - uploaded } else { 0 };
        let done = (goal - bytes_left) as f32 / goal as f32;
        (done as f64 * 100.0).max(0.0)
    } else {
        v.percent_done.max(0.0)
    }
}

fn torrent_icon(t: &Torrent) -> gio::Icon {
    let mime = match t.files.len() {
        0 => "unknown",
        1 => "file",
        _ => "folder",
    };
    mime_type_icon(mime)
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct TorrentRow {
        pub client: RefCell<Option<Client>>,
        pub compact: Cell<bool>,
        pub torrent: RefCell<Option<TorrentObject>>,
        pub handler: RefCell<Option<glib::SignalHandlerId>>,
        pub icon: gtk::Image,
        pub name: RefCell<Option<gtk::Label>>,
        pub progress: RefCell<Option<gtk::Label>>,
        pub bar: gtk::ProgressBar,
        pub status: RefCell<Option<gtk::Label>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TorrentRow {
        const NAME: &'static str = "TorrentRow";
        type Type = super::TorrentRow;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::BoxLayout>();
            klass.set_css_name("torrent-row");
        }
    }

    impl ObjectImpl for TorrentRow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            if let Some(layout) = obj.layout_manager().and_downcast::<gtk::BoxLayout>() {
                layout.set_spacing(GUI_PAD as u32);
            }
            obj.set_margin_top(2);
            obj.set_margin_bottom(2);
        }

        fn dispose(&self) {
            self.obj().set_torrent(None);
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for TorrentRow {}
}

glib::wrapper! {
    pub struct TorrentRow(ObjectSubclass<imp::TorrentRow>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

fn row_label(scale: f64, bold: bool) -> gtk::Label {
    let label = gtk::Label::new(None);
    let attrs = pango::AttrList::new();
    if scale != 1.0 {
        attrs.insert(pango::AttrFloat::new_scale(scale));
    }
    if bold {
        attrs.insert(pango::AttrInt::new_weight(pango::Weight::Bold));
    }
    label.set_attributes(Some(&attrs));
    label.set_xalign(0.0);
    label.set_ellipsize(pango::EllipsizeMode::End);
    label.set_single_line_mode(true);
    label
}

/// errors in red, paused torrents dimmed
fn set_text_style(label: &gtk::Label, t: &Torrent, v: &TorrentValues) {
    if t.error != 0 {
        label.add_css_class("error");
    } else {
        label.remove_css_class("error");
    }
    label.set_opacity(if t.error == 0 && v.flags & TORRENT_FLAG_PAUSED != 0 { 0.55 } else { 1.0 });
}

impl TorrentRow {
    pub fn new(client: &Client, compact: bool) -> Self {
        let row: Self = glib::Object::new();
        let imp = row.imp();
        *imp.client.borrow_mut() = Some(client.clone());
        imp.compact.set(compact);

        imp.icon.set_parent(&row);
        imp.bar.set_valign(gtk::Align::Center);

        if compact {
            // icon, name, status, a short progress bar
            imp.icon.set_icon_size(gtk::IconSize::Normal);

            let name = row_label(1.0, false);
            name.set_hexpand(true);
            name.set_parent(&row);

            let status = row_label(SMALL_SCALE, false);
            status.set_ellipsize(pango::EllipsizeMode::None);
            status.set_parent(&row);

            imp.bar.set_show_text(true);
            imp.bar.set_size_request(BAR_WIDTH, -1);
            imp.bar.set_parent(&row);

            *imp.name.borrow_mut() = Some(name);
            *imp.status.borrow_mut() = Some(status);
        } else {
            // icon, and then name, progress, progress bar and status lines
            let bx = gtk::Box::new(gtk::Orientation::Vertical, 0);
            imp.icon.set_icon_size(gtk::IconSize::Large);
            imp.icon.set_valign(gtk::Align::Center);

            let name = row_label(1.0, true);
            bx.append(&name);
            let progress = row_label(SMALL_SCALE, false);
            bx.append(&progress);
            imp.bar.set_margin_top(GUI_PAD_SMALL);
            imp.bar.set_margin_bottom(GUI_PAD_SMALL);
            bx.append(&imp.bar);
            let status = row_label(SMALL_SCALE, false);
            bx.append(&status);

            bx.set_hexpand(true);
            bx.set_parent(&row);

            *imp.name.borrow_mut() = Some(name);
            *imp.progress.borrow_mut() = Some(progress);
            *imp.status.borrow_mut() = Some(status);
        }
        row
    }

    fn refresh(&self) {
        let imp = self.imp();
        let Some(obj) = imp.torrent.borrow().clone() else { return };
        let Some(client) = imp.client.borrow().clone() else { return };
        let t = obj.torrent();
        let v = obj.values();

        let percent_done = percent_done(&t, &v, &client);

        // paused and queued torrents are dimmed, unless they have an error
        let sensitive = v.flags & INACTIVE == 0 || t.error != 0;

        imp.icon.set_from_gicon(&torrent_icon(&t));
        imp.icon.set_sensitive(sensitive);

        if let Some(name) = imp.name.borrow().as_ref() {
            name.set_text(&v.name);
            set_text_style(name, &t, &v);
        }

        imp.bar.set_fraction(percent_done.clamp(0.0, 100.0) / 100.0);
        imp.bar.set_sensitive(sensitive);

        let status_text = if imp.compact.get() {
            imp.bar.set_text(Some(&format::progress_text(percent_done)));
            short_status_string(&t, &v)
        } else {
            if let Some(progress) = imp.progress.borrow().as_ref() {
                progress.set_text(&progress_string(&t, &v, &client));
                set_text_style(progress, &t, &v);
            }
            status_string(&t, &v)
        };

        if let Some(status) = imp.status.borrow().as_ref() {
            status.set_text(&status_text);
            set_text_style(status, &t, &v);
        }
    }

    pub fn set_torrent(&self, torrent: Option<&TorrentObject>) {
        let imp = self.imp();
        if imp.torrent.borrow().as_ref() == torrent {
            return;
        }
        if let (Some(old), Some(h)) = (imp.torrent.take(), imp.handler.take()) {
            old.disconnect(h);
        }
        if let Some(torrent) = torrent {
            // An update sets the serial last, so all the values have changed
            // by the time this is notified.
            let weak = self.downgrade();
            let h = torrent.connect_notify_local(Some("serial"), move |_, _| {
                if let Some(row) = weak.upgrade() {
                    row.refresh();
                }
            });
            *imp.torrent.borrow_mut() = Some(torrent.clone());
            *imp.handler.borrow_mut() = Some(h);
            self.refresh();
        }
    }
}
