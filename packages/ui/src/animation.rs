use std::{cell::RefCell, rc::Rc};

use futures_signals::signal::{Mutable, SignalExt};
use silkenweb::{
    clone,
    elements::html::DivBuilder,
    node::element::{Element, ElementBuilder},
    prelude::{HtmlElement, HtmlElementEvents, ParentBuilder},
    task::on_animation_frame,
    value::Sig,
};
use web_sys::DomRect;

pub trait AnimatedExpand {
    fn animated_expand<Elem>(
        self,
        child: impl FnMut() -> Elem + 'static,
        expanded: Mutable<bool>,
    ) -> Self
    where
        Elem: Into<Element>;
}

impl AnimatedExpand for DivBuilder {
    fn animated_expand<Elem>(
        self,
        mut child: impl FnMut() -> Elem + 'static,
        is_expanded: Mutable<bool>,
    ) -> Self
    where
        Elem: Into<Element>,
    {
        let style = Mutable::new(Some("".to_owned()));
        let delayed_is_expanded = Mutable::new(is_expanded.get());
        let initial_bounds: Rc<RefCell<Option<DomRect>>> = Rc::new(RefCell::new(None));
        let element = self.handle().dom_element();
        let delayed_is_expanded_signal = delayed_is_expanded.signal();

        self.style(Sig(style.signal_cloned()))
            .optional_child(Sig(is_expanded.signal().map({
                clone!(initial_bounds);
                move |expanded| {
                    initial_bounds.replace(Some(element.get_bounding_client_rect()));
                    delayed_is_expanded.set(expanded);
                    expanded.then(|| child().into())
                }
            })))
            .on_transitionend({
                clone!(style);
                move |_, _| style.set(None)
            })
            .effect_signal(delayed_is_expanded_signal, move |elem, expanded| {
                let final_bounds = elem.get_bounding_client_rect();
                let limit = if expanded { "max" } else { "min" };
                set_style_size(&style, limit, initial_bounds.borrow().as_ref().unwrap());

                on_animation_frame({
                    clone!(style);
                    move || set_style_size(&style, limit, &final_bounds)
                })
            })
    }
}

fn set_style_size(style: &Mutable<Option<String>>, limit: &str, bounds: &DomRect) {
    let width = bounds.width();
    let height = bounds.height();
    style.set(Some(format!(
        "overflow: hidden; {limit}-width: {width}px; {limit}-height: {height}px",
    )));
}
