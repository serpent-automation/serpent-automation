use std::rc::Rc;

use serpent_automation_executor::{library::Library, syntax_tree::parse, CODE};
use serpent_automation_ui::app;
use silkenweb::mount;

fn main() {
    let module = parse(CODE).unwrap();
    let library = Rc::new(Library::link(module));

    mount("app", app(&library));
}
