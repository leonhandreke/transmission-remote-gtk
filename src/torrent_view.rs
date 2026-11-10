//! The list of torrents: the columns of a TreeView in the classic style, a
//! list of TorrentRows in the Transmission styles (torrent-row.c).

use std::cell::{Cell, RefCell};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib, pango};

use crate::client::Client;
use crate::i18n::{ctr, ntr, sprintf, tr};
use crate::item::Item;
use crate::prefs;
use crate::protocol::{
    self as p, TORRENT_FLAG_CHECKING, TORRENT_FLAG_COMPLETE, TORRENT_FLAG_DOWNLOADING,
    TORRENT_FLAG_DOWNLOADING_WAIT, TORRENT_FLAG_PAUSED, TORRENT_FLAG_SEEDING,
    TORRENT_FLAG_SEEDING_WAIT, TORRENT_FLAG_WAITING_CHECK,
};
use crate::torrent_model::*;
use crate::tree_view::{self, COLUMN_EXTRA, COLUMN_HIDE_FROM_TOP_MENU, ColType, TreeView};
use crate::util;

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

    let reg = |t, c: usize, h: &str, id: &str, f| tv.reg_column(t, c as i32, h, id, f);
    let desc = reg(ColType::IconText, COL_NAME, &tr("Name"), "name", 0);
    desc.set_extra(COL_ICON as i32);
    reg(ColType::Size, COL_SIZEWHENDONE, &tr("Size"), "size", 0);
    reg(ColType::Prog, COL_PERCENTDONE, &tr("Done"), "done", 0);
    reg(ColType::Text, COL_STATUS, &tr("Status"), "status", 0);
    reg(ColType::NumGtZero, COL_SEEDS, &tr("Seeds"), "seeds", 0);
    reg(ColType::NumGtZero, COL_PEERS_TO_US, &tr("Sending"), "sending", COLUMN_EXTRA);
    reg(ColType::NumGtZero, COL_LEECHERS, &tr("Leechers"), "leechers", 0);
    reg(ColType::NumGtZero, COL_DOWNLOADS, &tr("Downloads"), "downloads", COLUMN_EXTRA);
    reg(
        ColType::NumGtZero,
        COL_PEERS_FROM_US,
        &tr("Receiving"),
        "connected-leechers",
        COLUMN_EXTRA,
    );
    reg(ColType::NumGtZero, COL_PEERS_CONNECTED, &tr("Connected"), "connected-peers", 0);
    let hidden = COLUMN_EXTRA | COLUMN_HIDE_FROM_TOP_MENU;
    reg(ColType::NumGtZero, COL_FROMPEX, &tr("PEX Peers"), "from-pex", hidden);
    reg(ColType::NumGtZero, COL_FROMDHT, &tr("DHT Peers"), "from-dht", hidden);
    reg(ColType::NumGtZero, COL_FROMTRACKERS, &tr("Tracker Peers"), "from-trackers", hidden);
    reg(ColType::NumGtZero, COL_FROMLTEP, &tr("LTEP Peers"), "from-ltep", hidden);
    reg(ColType::NumGtZero, COL_FROMRESUME, &tr("Resumed Peers"), "from-resume", hidden);
    reg(ColType::NumGtZero, COL_FROMINCOMING, &tr("Incoming Peers"), "from-incoming", hidden);
    reg(ColType::Text, COL_PEER_SOURCES, &tr("Peers T/I/E/H/X/L/R"), "peer-sources", hidden);
    reg(ColType::Speed, COL_DOWNSPEED, &tr("Down Speed"), "down-speed", 0);
    reg(ColType::Speed, COL_UPSPEED, &tr("Up Speed"), "up-speed", 0);
    reg(ColType::Eta, COL_ETA, &tr("ETA"), "eta", 0);
    reg(ColType::Size, COL_UPLOADED, &tr("Uploaded"), "uploaded", 0);
    reg(ColType::Size, COL_DOWNLOADED, &tr("Downloaded"), "downloaded", 0);
    reg(ColType::Ratio, COL_RATIO, &tr("Ratio"), "ratio", 0);
    reg(ColType::Epoch, COL_ADDED, &tr("Added"), "added", 0);
    reg(ColType::Text, COL_TRACKERHOST, &tr("First Tracker"), "first-tracker", COLUMN_EXTRA);
    reg(ColType::Text, COL_DOWNLOADDIR, &tr("Location"), "download-dir", COLUMN_EXTRA);
    reg(ColType::Text, COL_ID, &tr("ID"), "id", COLUMN_EXTRA);
    reg(ColType::Prio, COL_BANDWIDTH_PRIORITY, &tr("Priority"), "priority", COLUMN_EXTRA);
    reg(
        ColType::NumGtEqZero,
        COL_QUEUE_POSITION,
        &tr("Queue Position"),
        "queue-position",
        COLUMN_EXTRA,
    );
    reg(ColType::Epoch, COL_DONE_DATE, &tr("Completed"), "done-date", COLUMN_EXTRA);
    reg(ColType::Epoch, COL_LASTACTIVE, &tr("Last Active"), "last-active", COLUMN_EXTRA);

    // No type ahead: typing filters the list instead (see MainWindow)

    tv.set_model(model);
    tv.setup_columns();
    tv.restore_sort();

    set_style(&tv, client, prefs.get_int(prefs::KEY_STYLE, prefs::GLOBAL));

    let weak = tv.downgrade();
    let c = client.downgrade();
    prefs.connect_changed(move |prefs, key| {
        if key == Some(prefs::KEY_STYLE)
            && let (Some(tv), Some(client)) = (weak.upgrade(), c.upgrade())
        {
            set_style(&tv, &client, prefs.get_int(prefs::KEY_STYLE, prefs::GLOBAL));
        }
    });

    tv
}

/// The IDs of the selected torrents
pub fn selected_ids(tv: &TreeView) -> Vec<i64> {
    tv.selected_items().iter().map(TorrentItemExt::torrent_id).collect()
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
            row.set_item(li.item().and_downcast::<Item>().as_ref());
        }
    });
    factory.connect_unbind(|_, obj| {
        let Some(li) = obj.downcast_ref::<gtk::ListItem>() else { return };
        if let Some(row) = li.child().and_downcast::<TorrentRow>() {
            row.set_item(None);
        }
    });
    tv.set_list_factory(Some(factory.upcast_ref()));
}

/* A row of the Transmission styles */

/// The values of the item, for the text helpers below
#[derive(Default)]
struct RowValues {
    flags: u32,
    file_count: u32,
    uploaded_ever: i64,
    size_when_done: i64,
    total_size: i64,
    downloaded: i64,
    have_valid: i64,
    have_unchecked: i64,
    up_speed: i64,
    down_speed: i64,
    peers_from_us: i64,
    web_seeds_to_us: i64,
    peers_to_us: i64,
    connected: i64,
    eta: i64,
    error: i64,
    seed_ratio_mode: i64,
    done: f64,
    metadata_percent_complete: f64,
    ratio: f64,
    seed_ratio_limit: f64,
    error_string: String,
}

impl RowValues {
    fn from_item(item: &Item) -> Self {
        Self {
            flags: item.uint(COL_FLAGS),
            file_count: item.uint(COL_FILECOUNT),
            uploaded_ever: item.int64(COL_UPLOADED),
            size_when_done: item.int64(COL_SIZEWHENDONE),
            total_size: item.int64(COL_TOTALSIZE),
            downloaded: item.int64(COL_DOWNLOADED),
            have_valid: item.int64(COL_HAVE_VALID),
            have_unchecked: item.int64(COL_HAVE_UNCHECKED),
            up_speed: item.int64(COL_UPSPEED),
            down_speed: item.int64(COL_DOWNSPEED),
            peers_from_us: item.int64(COL_PEERS_FROM_US),
            web_seeds_to_us: item.int64(COL_WEB_SEEDS_TO_US),
            peers_to_us: item.int64(COL_PEERS_TO_US),
            connected: item.int64(COL_PEERS_CONNECTED),
            eta: item.int64(COL_ETA),
            error: item.int64(COL_ERROR),
            seed_ratio_mode: item.int64(COL_SEED_RATIO_MODE),
            done: item.double(COL_PERCENTDONE),
            metadata_percent_complete: item.double(COL_METADATAPERCENTCOMPLETE),
            ratio: item.double(COL_RATIO),
            seed_ratio_limit: item.double(COL_SEED_RATIO_LIMIT),
            error_string: item
                .torrent_json()
                .and_then(|j| p::get_str(&j, p::FIELD_ERROR_STRING).map(str::to_string))
                .unwrap_or_default(),
        }
    }

    fn seed_ratio(&self, client: &Client) -> Option<f64> {
        if self.seed_ratio_mode == 0 && client.seed_ratio_limited() {
            Some(client.seed_ratio_limit())
        } else if self.seed_ratio_mode == 1 {
            Some(self.seed_ratio_limit)
        } else {
            None
        }
    }

    fn progress_string(&self, client: &Client) -> String {
        let have_total = self.have_unchecked + self.have_valid;
        let is_seed = self.have_valid >= self.total_size;
        let seed_ratio = self.seed_ratio(client);
        let size = util::format_size_or_none;

        let mut s = if self.flags & TORRENT_FLAG_DOWNLOADING != 0 {
            sprintf(
                &tr("%1$s of %2$s (%3$s)"),
                &[&size(have_total), &size(self.size_when_done), &util::format_percent(self.done)],
            )
        } else if !is_seed {
            match seed_ratio {
                Some(goal) => sprintf(
                    &tr("%1$s of %2$s (%3$s), uploaded %4$s (Ratio: %5$s Goal: %6$s)"),
                    &[
                        &size(have_total),
                        &size(self.total_size),
                        &util::format_percent(self.done),
                        &size(self.uploaded_ever),
                        &util::format_ratio(self.ratio),
                        &util::format_ratio(goal),
                    ],
                ),
                None => sprintf(
                    &tr("%1$s of %2$s (%3$s), uploaded %4$s (Ratio: %5$s)"),
                    &[
                        &size(have_total),
                        &size(self.total_size),
                        &util::format_percent(self.done),
                        &size(self.uploaded_ever),
                        &util::format_ratio(self.ratio),
                    ],
                ),
            }
        } else {
            match seed_ratio {
                Some(goal) => sprintf(
                    &tr("%1$s, uploaded %2$s (Ratio: %3$s Goal: %4$s)"),
                    &[
                        &size(self.total_size),
                        &size(self.uploaded_ever),
                        &util::format_ratio(self.ratio),
                        &util::format_ratio(goal),
                    ],
                ),
                None => sprintf(
                    &tr("%1$s, uploaded %2$s (Ratio: %3$s)"),
                    &[
                        &size(self.size_when_done),
                        &size(self.uploaded_ever),
                        &util::format_ratio(self.ratio),
                    ],
                ),
            }
        };

        // add time when downloading
        if self.flags & TORRENT_FLAG_DOWNLOADING != 0
            || (seed_ratio.is_some() && self.flags & TORRENT_FLAG_SEEDING != 0)
        {
            s.push_str(" - ");
            if self.eta < 0 {
                s.push_str(&tr("Remaining time unknown"));
            } else {
                s.push_str(&sprintf(&tr("%s remaining"), &[&util::format_time_long(self.eta)]));
            }
        }

        s
    }

    fn short_transfer_string(&self) -> String {
        let have_meta = self.file_count > 0;
        let have_up = have_meta && self.peers_from_us > 0;
        let have_down = have_meta && self.peers_to_us > 0;
        let down = util::format_speed_kbps((self.down_speed / util::SPEED_K) as f64);
        let up = util::format_speed_kbps((self.up_speed / util::SPEED_K) as f64);

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

    fn short_status_string(&self) -> String {
        let flags = self.flags;
        if flags & TORRENT_FLAG_PAUSED != 0 {
            if flags & TORRENT_FLAG_COMPLETE != 0 { tr("Finished") } else { tr("Paused") }
        } else if flags & TORRENT_FLAG_WAITING_CHECK != 0 {
            tr("Queued for verification")
        } else if flags & TORRENT_FLAG_DOWNLOADING_WAIT != 0 {
            tr("Queued for download")
        } else if flags & TORRENT_FLAG_SEEDING_WAIT != 0 {
            tr("Queued for seeding")
        } else if flags & TORRENT_FLAG_CHECKING != 0 {
            sprintf(&tr("Verifying data (%1$s tested)"), &[&util::format_percent(self.done)])
        } else if flags & (TORRENT_FLAG_DOWNLOADING | TORRENT_FLAG_SEEDING) != 0 {
            let mut s = String::new();
            if flags & !TORRENT_FLAG_DOWNLOADING != 0 {
                s.push_str(&sprintf(&tr("Ratio %s"), &[&util::format_ratio(self.ratio)]));
                s.push_str(", ");
            }
            s.push_str(&self.short_transfer_string());
            s
        } else {
            String::new()
        }
    }

    fn status_string(&self) -> String {
        let flags = self.flags;
        let mut s = if self.error != 0 {
            match self.error {
                1 => sprintf(&tr("Tracker gave a warning: \"%s\""), &[&self.error_string]),
                2 => sprintf(&tr("Tracker gave an error: \"%s\""), &[&self.error_string]),
                3 => sprintf(&tr("Error: \"%s\""), &[&self.error_string]),
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
            self.short_status_string()
        } else if flags & TORRENT_FLAG_DOWNLOADING != 0 {
            if self.file_count > 0 {
                let n = self.web_seeds_to_us + self.peers_to_us;
                sprintf(
                    &ntr(
                        "Downloading from %1$lu of %2$lu connected peer",
                        "Downloading from %1$lu of %2$lu connected peers",
                        n,
                    ),
                    &[&n, &(self.web_seeds_to_us + self.connected)],
                )
            } else {
                let n = self.connected + self.web_seeds_to_us;
                sprintf(
                    &ntr(
                        "Downloading metadata from %1$lu peer (%2$s done)",
                        "Downloading metadata from %1$lu peers (%2$s done)",
                        n,
                    ),
                    &[&n, &util::format_percent(self.metadata_percent_complete)],
                )
            }
        } else if flags & TORRENT_FLAG_SEEDING != 0 {
            sprintf(
                &ntr(
                    "Seeding to %1$lu of %2$lu connected peer",
                    "Seeding to %1$lu of %2$lu connected peers",
                    self.connected,
                ),
                &[&self.peers_from_us, &self.connected],
            )
        } else {
            String::new()
        };

        // (these are always true in the C version, which tests flags & ~flag)
        if flags & !TORRENT_FLAG_WAITING_CHECK != 0
            && flags & !TORRENT_FLAG_CHECKING != 0
            && flags & !TORRENT_FLAG_DOWNLOADING_WAIT != 0
            && flags & !TORRENT_FLAG_SEEDING_WAIT != 0
            && flags & !TORRENT_FLAG_PAUSED != 0
        {
            let t = self.short_transfer_string();
            if !t.is_empty() {
                s.push_str(" - ");
                s.push_str(&t);
            }
        }
        s
    }

    fn percent_done(&self, client: &Client) -> f64 {
        if self.flags & TORRENT_FLAG_SEEDING != 0 && self.seed_ratio(client).is_some() {
            let baseline = if self.downloaded != 0 { self.downloaded } else { self.size_when_done };
            let goal = (baseline as f64 * self.seed_ratio_limit) as i64;
            let bytes_left = if goal > self.uploaded_ever { goal - self.uploaded_ever } else { 0 };
            let done = (goal - bytes_left) as f32 / goal as f32;
            (done as f64 * 100.0).max(0.0)
        } else {
            self.done.max(0.0)
        }
    }

    fn icon(&self) -> gio::Icon {
        let mime = match self.file_count {
            0 => "unknown",
            1 => "file",
            _ => "folder",
        };
        mime_type_icon(mime)
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct TorrentRow {
        pub client: RefCell<Option<Client>>,
        pub compact: Cell<bool>,
        pub item: RefCell<Option<Item>>,
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
            self.obj().set_item(None);
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
fn set_text_style(label: &gtk::Label, v: &RowValues) {
    if v.error != 0 {
        label.add_css_class("error");
    } else {
        label.remove_css_class("error");
    }
    label.set_opacity(if v.error == 0 && v.flags & TORRENT_FLAG_PAUSED != 0 { 0.55 } else { 1.0 });
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
        let Some(item) = imp.item.borrow().clone() else { return };
        let Some(client) = imp.client.borrow().clone() else { return };
        let v = RowValues::from_item(&item);

        let active = v.flags & !TORRENT_FLAG_PAUSED != 0
            && v.flags & !TORRENT_FLAG_DOWNLOADING_WAIT != 0
            && v.flags & !TORRENT_FLAG_SEEDING_WAIT != 0;
        let sensitive = active || v.error != 0;
        let percent_done = v.percent_done(&client);

        imp.icon.set_from_gicon(&v.icon());
        imp.icon.set_sensitive(sensitive);

        if let Some(name) = imp.name.borrow().as_ref() {
            name.set_text(&item.string(COL_NAME).unwrap_or_default());
            set_text_style(name, &v);
        }

        imp.bar.set_fraction(percent_done.clamp(0.0, 100.0) / 100.0);
        imp.bar.set_sensitive(sensitive);

        let status_text = if imp.compact.get() {
            imp.bar.set_text(Some(&sprintf(
                &ctr("progress bar label", "%d %%"),
                &[&(percent_done as i64)],
            )));
            v.short_status_string()
        } else {
            if let Some(progress) = imp.progress.borrow().as_ref() {
                progress.set_text(&v.progress_string(&client));
                set_text_style(progress, &v);
            }
            v.status_string()
        };

        if let Some(status) = imp.status.borrow().as_ref() {
            status.set_text(&status_text);
            set_text_style(status, &v);
        }
    }

    pub fn set_item(&self, item: Option<&Item>) {
        let imp = self.imp();
        if imp.item.borrow().as_ref() == item {
            return;
        }
        if let (Some(old), Some(h)) = (imp.item.take(), imp.handler.take()) {
            old.disconnect(h);
        }
        if let Some(item) = item {
            // An update sets the update serial last, so all the values have
            // changed by the time this is notified.
            let weak = self.downgrade();
            let h = item.connect_column_changed(COL_UPDATESERIAL, move |_| {
                if let Some(row) = weak.upgrade() {
                    row.refresh();
                }
            });
            *imp.item.borrow_mut() = Some(item.clone());
            *imp.handler.borrow_mut() = Some(h);
            self.refresh();
        }
    }
}
