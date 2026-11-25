//! Settings objects: GObjects whose properties are the settings a dialog
//! edits. The dialog's template binds its widgets to them (`active: bind
//! settings.dht-enabled bidirectional;`), so a setting is named once there.
//!
//! `settings_object!` makes one from the fields of a struct (like
//! `Session`): it holds the struct, loaded with `load()`, and `changes()`
//! are the fields that differ from what was loaded, to send to the server.

use gtk::glib;

/// The type of a field of the struct of a settings object, and that of its
/// property: the field's own type, or T for an optional T (None shows as
/// T's default, and setting the property makes it Some)
pub trait Setting {
    type Property;
    fn get(&self) -> Self::Property;
    fn set(&mut self, value: Self::Property);
}

macro_rules! plain_settings {
    ($($ty:ty),*) => {$(
        impl Setting for $ty {
            type Property = $ty;
            fn get(&self) -> $ty {
                self.clone()
            }
            fn set(&mut self, value: $ty) {
                *self = value;
            }
        }
    )*};
}

plain_settings!(bool, i64, f64, String, Vec<String>);

impl<T: Setting<Property = T> + Clone + Default> Setting for Option<T> {
    type Property = T;
    fn get(&self) -> T {
        self.clone().unwrap_or_default()
    }
    fn set(&mut self, value: T) {
        *self = Some(value);
    }
}

/// Defines a settings object holding a `$values` struct, whose properties
/// are the fields listed: `field: Type` is the property "field" (with
/// dashes), of the Setting type of the field's Type. The private module is
/// named `$imp`. `changes()` names the fields like serde does by default.
macro_rules! settings_object {
    (
        $(#[$doc:meta])*
        $vis:vis struct $name:ident($type_name:literal, $values:ty) in mod $imp:ident {
            $($field:ident: $ty:ty,)*
        }
    ) => {
        mod $imp {
            use std::cell::RefCell;
            use std::sync::OnceLock;

            use gtk::glib;
            use gtk::glib::prelude::*;
            use gtk::glib::subclass::prelude::*;

            use super::*;
            use $crate::dialogs::settings::Setting;

            #[derive(Default)]
            pub struct $name {
                /// The values, as edited
                pub values: RefCell<$values>,
                /// The values loaded
                pub loaded: RefCell<$values>,
            }

            #[glib::object_subclass]
            impl ObjectSubclass for $name {
                const NAME: &'static str = $type_name;
                type Type = super::$name;
            }

            impl ObjectImpl for $name {
                fn properties() -> &'static [glib::ParamSpec] {
                    static PROPERTIES: OnceLock<Vec<glib::ParamSpec>> = OnceLock::new();
                    PROPERTIES.get_or_init(|| {
                        vec![$(<<$ty as Setting>::Property as glib::HasParamSpec>::param_spec_builder()(
                            &stringify!($field).replace('_', "-"),
                        )
                        .build(),)*]
                    })
                }

                // the IDs are the properties' positions, from 1
                fn property(&self, id: usize, _pspec: &glib::ParamSpec) -> glib::Value {
                    let getters: &[fn(&$values) -> glib::Value] =
                        &[$(|v| <$ty as Setting>::get(&v.$field).to_value(),)*];
                    getters[id - 1](&self.values.borrow())
                }

                fn set_property(&self, id: usize, value: &glib::Value, _pspec: &glib::ParamSpec) {
                    let setters: &[fn(&mut $values, &glib::Value)] = &[$(|v, value| {
                        <$ty as Setting>::set(&mut v.$field, value.get().expect("the property's type"))
                    },)*];
                    setters[id - 1](&mut self.values.borrow_mut(), value);
                }
            }
        }

        glib::wrapper! {
            $(#[$doc])*
            $vis struct $name(ObjectSubclass<$imp::$name>);
        }

        impl $name {
            /// Shows values: the properties change, and the changes are
            /// relative to them
            pub fn load(&self, values: $values) {
                let imp = self.imp();
                imp.values.replace(values.clone());
                imp.loaded.replace(values);
                let _guard = self.freeze_notify();
                for pspec in self.list_properties() {
                    self.notify_by_pspec(&pspec);
                }
            }

            /// The fields that changed since load(), by name
            pub fn changes(&self) -> $crate::json::Obj {
                let (values, loaded) = (self.imp().values.borrow(), self.imp().loaded.borrow());
                let mut changes = $crate::json::Obj::new();
                $(if values.$field != loaded.$field {
                    changes.insert(stringify!($field).into(), serde_json::json!(values.$field));
                })*
                changes
            }
        }
    };
}

pub(crate) use settings_object;

/// Closures for the templates' bindings
pub struct Closures;

#[gtk::template_callbacks(functions)]
impl Closures {
    #[template_callback]
    fn either(a: bool, b: bool) -> bool {
        a || b
    }

    #[template_callback]
    fn both(a: bool, b: bool) -> bool {
        a && b
    }
}

/// The value of a settings object's property, from a binding's transform
/// function (the source is the settings object)
pub fn source_property<T: for<'a> glib::value::FromValue<'a> + 'static>(
    binding: &glib::Binding,
    property: &str,
) -> T {
    use gtk::prelude::*;
    binding.source().expect("the settings object").property(property)
}
