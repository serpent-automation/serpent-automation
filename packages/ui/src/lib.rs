use std::rc::Rc;

use serpent_automation_executor::library::Library;
use serpent_automation_frontend::{call_tree::CallTree, ServerConnection};
use silkenweb::{
    node::element::ChildElement,
    prelude::{Element, ParentElement},
    task::spawn_local,
};
use silkenweb_bootstrap::column;
use thread_view::ThreadView;

mod animation;
mod call_tree_view;
mod source_view;
mod splitter;
mod thread_view;
mod css {
    silkenweb::css!(path = "serpent-automation.css");

    pub use class::*;
}

macro_rules! component {
    ($path:literal) => {
        silkenweb::css!(
            path = concat("css/", $path, ".css"),
            auto_mount,
            transpile = (modules)
        );
    };
}

use component;

pub fn app(library: &Rc<Library>) -> impl ChildElement {
    let main_id = library.main_id().unwrap();
    let call_tree = CallTree::root(main_id, library);

    spawn_local(call_tree.update_run_state(ServerConnection::default()));

    column()
        .class(css::HEIGHT_FULLSCREEN)
        .child(ThreadView::new(call_tree))
}
