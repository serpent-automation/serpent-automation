use std::rc::Rc;

use serpent_automation_executor::library::Library;
use serpent_automation_frontend::StackFrameStates;
use silkenweb::{
    node::{element::ElementBuilder, Node},
    prelude::ParentBuilder,
};
use silkenweb_bootstrap::{
    row,
    utility::{Align, Overflow, SetFlex, SetOverflow, SetSpacing, Size::Size3},
};
use thread_view::ThreadView;

mod thread_view;
mod css {
    silkenweb::css_classes!(visibility: pub, path: "serpent-automation.scss");
}

pub fn app(library: &Rc<Library>, stack_frame_states: &StackFrameStates) -> impl Into<Node> {
    let main_id = library.main_id().unwrap();

    row()
        .margin(Some(Size3))
        .class(css::FLOW_DIAGRAMS_CONTAINER)
        .align_items(Align::Start)
        .overflow(Overflow::Auto)
        .child(ThreadView::new(main_id, library, stack_frame_states))
}