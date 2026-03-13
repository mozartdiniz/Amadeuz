use glib::prelude::*;
use glib::subclass::prelude::*;
use glib::Properties;

mod imp {
    use super::*;
    use std::cell::RefCell;

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::AmzFolder)]
    pub struct AmzFolder {
        #[property(get, set)]
        pub id: RefCell<String>,
        #[property(get, set)]
        pub name: RefCell<String>,
        #[property(get, set)]
        pub created_at: RefCell<i64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AmzFolder {
        const NAME: &'static str = "AmzFolder";
        type Type = super::AmzFolder;
        type ParentType = glib::Object;
    }

    #[glib::derived_properties]
    impl ObjectImpl for AmzFolder {}
}

glib::wrapper! {
    pub struct AmzFolder(ObjectSubclass<imp::AmzFolder>);
}

impl AmzFolder {
    pub fn new(id: &str, name: &str, created_at: i64) -> Self {
        glib::Object::builder()
            .property("id", id)
            .property("name", name)
            .property("created-at", created_at)
            .build()
    }
}
