//! The General page of the torrent details: the selected torrent's state
//! and statistics (trg-general-panel.c).

use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, glib};

use crate::format;
use crate::i18n::tr;
use crate::item::Item;
use crate::json::{Obj, get_bool, get_f64, get_i64, get_str};
use crate::protocol as p;
use crate::torrent_model as tm;

mod imp {
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/io/github/TransmissionRemoteGtk/ui/general-panel.ui")]
    pub struct GeneralPanel {
        #[template_child]
        pub gen_name_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_size_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_completed_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_seeders_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_leechers_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_status_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_priority_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_eta_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_downloaded_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_uploaded_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_corrupted_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_down_rate_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_up_rate_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_ratio_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_limit_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_completedat_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_downloaddir_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_comment_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_hash_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub gen_error_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub error_key_label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GeneralPanel {
        const NAME: &'static str = "TrgGeneralPanel";
        type Type = super::GeneralPanel;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for GeneralPanel {}
    impl WidgetImpl for GeneralPanel {}
    impl BoxImpl for GeneralPanel {}
}

glib::wrapper! {
    pub struct GeneralPanel(ObjectSubclass<imp::GeneralPanel>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl GeneralPanel {
    fn labels(&self) -> Vec<gtk::Label> {
        let i = self.imp();
        vec![
            i.gen_name_label.get(),
            i.gen_size_label.get(),
            i.gen_completed_label.get(),
            i.gen_seeders_label.get(),
            i.gen_leechers_label.get(),
            i.gen_status_label.get(),
            i.gen_priority_label.get(),
            i.gen_eta_label.get(),
            i.gen_downloaded_label.get(),
            i.gen_uploaded_label.get(),
            i.gen_corrupted_label.get(),
            i.gen_down_rate_label.get(),
            i.gen_up_rate_label.get(),
            i.gen_ratio_label.get(),
            i.gen_limit_label.get(),
            i.gen_completedat_label.get(),
            i.gen_downloaddir_label.get(),
            i.gen_comment_label.get(),
            i.gen_hash_label.get(),
            i.gen_error_label.get(),
            i.error_key_label.get(),
        ]
    }

    pub fn clear(&self) {
        for l in self.labels() {
            l.set_text("");
        }
    }

    pub fn update(&self, t: &Obj, item: &Item) {
        let i = self.imp();
        let seeders = item.int64(tm::COL_SEEDS);
        let leechers = item.int64(tm::COL_LEECHERS);
        let status = item.string(tm::COL_STATUS).unwrap_or_default();

        i.gen_size_label.set_text(&format::size(get_i64(t, p::FIELD_SIZEWHENDONE)));

        let rate = |rate: i64, limited: &str, limit: &str| {
            let s = format::speed(rate);
            if get_bool(t, limited) {
                format!("{s} [{}]", format::speed_kbps(get_i64(t, limit) as f64))
            } else {
                s
            }
        };
        i.gen_down_rate_label.set_text(&rate(
            get_i64(t, p::FIELD_RATEDOWNLOAD),
            p::FIELD_DOWNLOAD_LIMITED,
            p::FIELD_DOWNLOAD_LIMIT,
        ));
        i.gen_up_rate_label.set_text(&rate(
            get_i64(t, p::FIELD_RATEUPLOAD),
            p::FIELD_UPLOAD_LIMITED,
            p::FIELD_UPLOAD_LIMIT,
        ));

        i.gen_corrupted_label.set_text(&format::size(get_i64(t, p::FIELD_CORRUPTEVER)));
        let uploaded = get_i64(t, p::FIELD_UPLOADEDEVER);
        i.gen_uploaded_label.set_text(&format::size(uploaded));
        i.gen_hash_label.set_text(get_str(t, p::FIELD_HASH_STRING).unwrap_or(""));

        let have_valid = get_i64(t, p::FIELD_HAVEVALID);
        i.gen_downloaded_label.set_text(&format::size(get_i64(t, p::FIELD_DOWNLOADEDEVER)));

        if uploaded > 0 && have_valid > 0 {
            i.gen_ratio_label.set_text(&format::ratio(uploaded as f64 / have_valid as f64));
        } else {
            i.gen_ratio_label.set_text(&tr("N/A"));
        }

        i.gen_limit_label.set_text(&format::ratio(get_f64(t, p::FIELD_SEED_RATIO_LIMIT)));

        let completed_at = get_i64(t, p::FIELD_DONE_DATE);
        i.gen_completedat_label.set_text(&if completed_at > 0 {
            format::date_time(completed_at)
        } else {
            String::new()
        });

        let privacy =
            if get_bool(t, p::FIELD_ISPRIVATE) { tr("(Private)") } else { tr("(Public)") };
        i.gen_status_label.set_text(&format!("{status} {privacy}"));

        match get_i64(t, p::FIELD_BANDWIDTH_PRIORITY) {
            p::TR_PRI_LOW => i.gen_priority_label.set_text(&tr("Low")),
            p::TR_PRI_NORMAL => i.gen_priority_label.set_text(&tr("Normal")),
            p::TR_PRI_HIGH => i.gen_priority_label.set_text(&tr("High")),
            _ => {}
        }

        i.gen_completed_label.set_text(&format::percent(p::torrent_get_percent_done(t)));
        i.gen_name_label.set_text(p::torrent_get_name(t));
        i.gen_downloaddir_label.set_text(get_str(t, p::FIELD_DOWNLOAD_DIR).unwrap_or(""));
        i.gen_comment_label
            .set_markup(&format::add_links_to_text(get_str(t, p::FIELD_COMMENT).unwrap_or("")));

        let error = get_str(t, p::FIELD_ERROR_STRING).unwrap_or("");
        if !error.is_empty() {
            i.gen_error_label.set_markup(&format!(
                "<span fgcolor=\"red\">{}</span>",
                glib::markup_escape_text(error)
            ));
            i.error_key_label.set_markup(&format!(
                "<span font_weight=\"bold\" fgcolor=\"red\">{}</span>",
                glib::markup_escape_text(&tr("Error"))
            ));
        } else {
            i.gen_error_label.set_text("");
            i.error_key_label.set_text("");
        }

        let eta = get_i64(t, p::FIELD_ETA);
        i.gen_eta_label.set_text(&if eta > 0 { format::time_long(eta) } else { tr("N/A") });

        i.gen_seeders_label.set_text(&seeders.max(0).to_string());
        i.gen_leechers_label.set_text(&leechers.max(0).to_string());
    }
}
