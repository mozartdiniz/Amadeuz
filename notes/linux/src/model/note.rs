use glib::prelude::*;
use glib::subclass::prelude::*;
use glib::Properties;

/// GObject wrapper around a note.
/// Properties are used so GTK list widgets can bind to them directly.
mod imp {
    use super::*;
    use std::cell::RefCell;

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::AmzNote)]
    pub struct AmzNote {
        #[property(get, set)]
        pub id: RefCell<String>,
        #[property(get, set)]
        pub title: RefCell<String>,
        #[property(get, set)]
        pub content: RefCell<String>,
        #[property(get, set)]
        pub folder_id: RefCell<String>,
        #[property(get, set)]
        pub updated_at: RefCell<i64>,
        #[property(get, set)]
        pub created_at: RefCell<i64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AmzNote {
        const NAME: &'static str = "AmzNote";
        type Type = super::AmzNote;
        type ParentType = glib::Object;
    }

    #[glib::derived_properties]
    impl ObjectImpl for AmzNote {}
}

glib::wrapper! {
    pub struct AmzNote(ObjectSubclass<imp::AmzNote>);
}

impl AmzNote {
    pub fn new(
        id: &str,
        title: &str,
        content: &str,
        folder_id: &str,
        updated_at: i64,
        created_at: i64,
    ) -> Self {
        glib::Object::builder()
            .property("id", id)
            .property("title", title)
            .property("content", content)
            .property("folder-id", folder_id)
            .property("updated-at", updated_at)
            .property("created-at", created_at)
            .build()
    }
}
