//! The General page of the torrent details: the selected torrent's state and
//! statistics.

use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, glib};

use crate::format;
use crate::i18n::tr;
use crate::protocol::{self as p, Torrent};
use crate::torrent_object::TorrentValues;

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

    pub fn update(&self, t: &Torrent, v: &TorrentValues) {
        let i = self.imp();
        let (seeders, leechers, status) = (v.seeders, v.leechers, &v.status);

        i.gen_size_label.set_text(&format::size(t.size_when_done));

        let rate = |rate: i64, limited: bool, limit: i64| {
            let s = format::speed(rate);
            if limited { format!("{s} [{}]", format::speed_kbps(limit as f64)) } else { s }
        };
        i.gen_down_rate_label.set_text(&rate(
            t.rate_download,
            t.download_limited,
            t.download_limit,
        ));
        i.gen_up_rate_label.set_text(&rate(t.rate_upload, t.upload_limited, t.upload_limit));

        i.gen_corrupted_label.set_text(&format::size(t.corrupt_ever));
        i.gen_uploaded_label.set_text(&format::size(t.uploaded_ever));
        i.gen_hash_label.set_text(&t.hash_string);
        i.gen_downloaded_label.set_text(&format::size(t.downloaded_ever));

        if t.uploaded_ever > 0 && t.have_valid > 0 {
            i.gen_ratio_label
                .set_text(&format::ratio(t.uploaded_ever as f64 / t.have_valid as f64));
        } else {
            i.gen_ratio_label.set_text(&tr("N/A"));
        }

        i.gen_limit_label.set_text(&format::ratio(t.seed_ratio_limit));

        i.gen_completedat_label.set_text(&if t.done_date > 0 {
            format::date_time(t.done_date)
        } else {
            String::new()
        });

        let privacy = if t.is_private { tr("(Private)") } else { tr("(Public)") };
        i.gen_status_label.set_text(&format!("{status} {privacy}"));

        match t.bandwidth_priority {
            p::TR_PRI_LOW => i.gen_priority_label.set_text(&tr("Low")),
            p::TR_PRI_NORMAL => i.gen_priority_label.set_text(&tr("Normal")),
            p::TR_PRI_HIGH => i.gen_priority_label.set_text(&tr("High")),
            _ => {}
        }

        i.gen_completed_label.set_text(&format::percent(t.done_percent()));
        i.gen_name_label.set_text(&t.name);
        i.gen_downloaddir_label.set_text(&t.download_dir);
        i.gen_comment_label.set_markup(&format::add_links_to_text(&t.comment));

        let error = &t.error_string;
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

        let eta = t.eta;
        i.gen_eta_label.set_text(&if eta > 0 { format::time_long(eta) } else { tr("N/A") });

        i.gen_seeders_label.set_text(&seeders.max(0).to_string());
        i.gen_leechers_label.set_text(&leechers.max(0).to_string());
    }
}
