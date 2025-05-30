use silkenweb::{
    clone, custom_html_element,
    dom::Dom,
    element_slot, element_slot_single,
    elements::{CustomEvent, ElementEvents},
    node::element::Element,
    StrAttribute, Value,
};
use strum::AsRefStr;
use wasm_bindgen::{prelude::wasm_bindgen, JsValue, UnwrapThrowExt};
use web_sys::js_sys::Reflect;

use crate::{
    button::Button,
    icon::{self, Icon},
    link::Link,
    Highlight, ItemType,
};

custom_html_element!(
    container("ui5-menu") = {
        dom_type: web_sys::HtmlElement;

        attributes {
            header_text: String,
            open: bool,
            horizontal_align: Align,
            loading: bool,
            loading_delay: usize,
            opener: String,
        };

        events {
            item_click: CustomEvent<ItemClickEvent>,
            before_open: CustomEvent<BeforeOpenEvent>,
            open: web_sys::CustomEvent,
            before_close: CustomEvent<BeforeCloseEvent>,
            close: web_sys::CustomEvent,
        };
    }
);

impl<D: Dom> Container<D> {
    pub fn set_opener(&self, element: &web_sys::HtmlElement) {
        assert!(Reflect::set(
            &self.handle().dom_element(),
            &JsValue::from_str("opener"),
            element
        )
        .unwrap_throw());
    }
}

pub trait Child {}
impl Child for Item {}
impl Child for Separator {}

element_slot!(container, item, None::<String>, impl Child);

custom_html_element!(
    item("ui5-menu-item") = {
        dom_type: web_sys::HtmlElement;

        attributes {
            text: String,
            additional_text: String,
            icon: icon::Name,
            disabled: bool,
            loading: bool,
            loading_delay: usize,
            accessible_name: String,
            tooltip: String,
            r#type: ItemType,
            navigated: bool,
            highlight: Highlight,
            selected: bool
        };

        events {
            before_open: CustomEvent<BeforeOpenEvent>,
            open: web_sys::CustomEvent,
            before_close: CustomEvent<BeforeCloseEvent>,
            close: web_sys::CustomEvent,
            detail_click: web_sys::CustomEvent,
        };
    }
);

impl<D: Dom> Item<D> {
    pub fn on_select(self, mut handler: impl FnMut() + Clone + 'static) -> Self {
        self.on_click({
            clone!(mut handler);
            move |_, _| handler()
        })
        .on_keydown(move |ev, _| {
            if ev.key() == "Enter" {
                handler()
            }
        })
    }
}

pub trait EndContent {}
impl EndContent for Button {}
impl EndContent for Link {}
impl EndContent for Icon {}

element_slot!(item, item, None::<String>, impl Child);
element_slot!(item, end_content, "endContent", impl EndContent);
element_slot_single!(item, delete_button, "deleteButton", Button);

custom_html_element!(
    separator("ui5-menu-separator") = {
        dom_type: web_sys::HtmlElement;
    }
);

#[derive(Copy, Clone, Eq, PartialEq, AsRefStr, StrAttribute, Value)]
pub enum Align {
    Center,
    Start,
    End,
    Stretch,
}

#[wasm_bindgen]
extern "C" {
    pub type ItemClickEvent;

    #[wasm_bindgen(method, getter, structural)]
    pub fn item(this: &ItemClickEvent) -> web_sys::HtmlElement;

    #[wasm_bindgen(method, getter, structural)]
    pub fn text(this: &ItemClickEvent) -> String;

    pub type BeforeOpenEvent;

    #[wasm_bindgen(method, getter, structural)]
    pub fn item(this: &BeforeOpenEvent) -> web_sys::HtmlElement;

    pub type BeforeCloseEvent;

    #[wasm_bindgen(method, getter = escPressed, structural)]
    pub fn esc_pressed(this: &BeforeCloseEvent) -> bool;
}
