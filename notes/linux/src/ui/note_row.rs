use adw::subclass::prelude::*;
use gtk::prelude::*;
use gtk::{glib, CompositeTemplate, TemplateChild};

use crate::model::AmzNote;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/com/amadeuz/Notes/ui/note_row.ui")]
    pub struct AmzNoteRow {
        #[template_child] pub title_label:   TemplateChild<gtk::Label>,
        #[template_child] pub date_label:    TemplateChild<gtk::Label>,
        #[template_child] pub preview_label: TemplateChild<gtk::Label>,
        #[template_child] pub folder_label:  TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AmzNoteRow {
        const NAME: &'static str = "AmzNoteRow";
        type Type = super::AmzNoteRow;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AmzNoteRow {}
    impl WidgetImpl for AmzNoteRow {}
    impl BoxImpl for AmzNoteRow {}
}

glib::wrapper! {
    pub struct AmzNoteRow(ObjectSubclass<imp::AmzNoteRow>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl AmzNoteRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn bind_note(&self, note: &AmzNote, folder_name: &str) {
        let imp = self.imp();

        let title = note.title();
        imp.title_label
            .set_text(if title.is_empty() { "Untitled" } else { &title });

        imp.date_label.set_text(&format_timestamp(note.updated_at()));

        let content = note.content();
        let preview: String = content.chars().take(80).collect();
        let preview = preview.replace('\n', " ");
        imp.preview_label.set_text(&preview);

        imp.folder_label.set_text(folder_name);
        imp.folder_label.set_visible(!folder_name.is_empty());
    }
}

impl Default for AmzNoteRow {
    fn default() -> Self {
        Self::new()
    }
}

fn format_timestamp(ms: i64) -> String {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    if ms == 0 {
        return String::new();
    }

    let t = UNIX_EPOCH + Duration::from_millis(ms as u64);
    let now = SystemTime::now();

    let Ok(elapsed) = now.duration_since(t) else {
        return String::new();
    };

    let secs = elapsed.as_secs();
    if secs < 60 {
        "Just now".into()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else if secs < 86400 * 7 {
        format!("{}d ago", secs / 86400)
    } else {
        // Format as YYYY-MM-DD using the timestamp.
        let secs_since_epoch = ms / 1000;
        // Simple calculation for display — no chrono needed.
        let days = secs_since_epoch / 86400;
        let year = 1970 + days / 365;
        let month = (days % 365) / 30 + 1;
        let day = (days % 365) % 30 + 1;
        format!("{year:04}-{month:02}-{day:02}")
    }
}
