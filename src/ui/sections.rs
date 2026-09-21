use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

mod imp {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct Sections {
        pub(super) items: RefCell<Vec<gtk::StringObject>>,

        pub(super) sections: RefCell<Vec<(String, u32)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Sections {
        const NAME: &'static str = "NumaSections";
        type Type = super::Sections;
        type Interfaces = (gio::ListModel, gtk::SectionModel);
    }

    impl ObjectImpl for Sections {}

    impl ListModelImpl for Sections {
        fn item_type(&self) -> glib::Type {
            gtk::StringObject::static_type()
        }

        fn n_items(&self) -> u32 {
            self.items.borrow().len() as u32
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            self.items.borrow().get(position as usize).map(|item| item.clone().upcast())
        }
    }

    impl SectionModelImpl for Sections {
        fn section(&self, position: u32) -> (u32, u32) {
            let sections = self.sections.borrow();
            let at = sections.iter().rposition(|(_, start)| *start <= position).unwrap_or(0);
            let start = sections.get(at).map_or(0, |(_, start)| *start);
            let end = sections.get(at + 1).map_or(self.n_items(), |(_, start)| *start);
            (start, end)
        }
    }
}

glib::wrapper! {
    pub struct Sections(ObjectSubclass<imp::Sections>)
        @implements gio::ListModel, gtk::SectionModel;
}

impl Sections {

    pub fn new<S: AsRef<str>>(sections: &[(S, Vec<String>)]) -> Self {
        let model: Self = glib::Object::new();
        let imp = model.imp();
        let mut items = Vec::new();
        for (title, labels) in sections.iter().filter(|(_, labels)| !labels.is_empty()) {
            imp.sections.borrow_mut().push((title.as_ref().to_string(), items.len() as u32));
            items.extend(labels.iter().map(|label| gtk::StringObject::new(label)));
        }
        *imp.items.borrow_mut() = items;
        model
    }

    pub fn title(&self, position: u32) -> String {
        let sections = self.imp().sections.borrow();
        sections.iter().find(|(_, start)| *start == position).map(|(title, _)| title.clone()).unwrap_or_default()
    }

    pub fn section_count(&self) -> usize {
        self.imp().sections.borrow().len()
    }

    pub fn is<S: AsRef<str>>(&self, sections: &[(S, Vec<String>)]) -> bool {
        let other = Self::new(sections);
        *self.imp().sections.borrow() == *other.imp().sections.borrow()
            && self.imp().items.borrow().iter().map(|item| item.string()).eq(other.imp().items.borrow().iter().map(|item| item.string()))
    }
}
