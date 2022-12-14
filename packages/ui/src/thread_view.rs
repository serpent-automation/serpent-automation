use std::rc::Rc;

use derive_more::Into;
use futures_signals::signal::{Mutable, SignalExt};
use serpent_automation_executor::{
    library::{FunctionId, Library},
    syntax_tree::SrcSpan,
    CODE,
};
use silkenweb::{
    clone,
    elements::html::{self, div, DivBuilder},
    node::Node,
    prelude::{ElementEvents, ParentBuilder},
    value::Sig,
    Value,
};
use silkenweb_bootstrap::{
    column,
    tab_bar::{tab_bar, Style},
    utility::{
        Active, Display, Overflow, SetDisplay, SetGap, SetOverflow, SetSpacing, Size::Size3,
    },
};

use crate::{
    call_tree_view::{CallTree, CallTreeActions},
    source_view::{Editor, SourceView},
    ViewCallStates,
};

#[derive(Into, Value)]
pub struct ThreadView(Node);

impl ThreadView {
    pub fn new(
        fn_id: FunctionId,
        library: &Rc<Library>,
        view_call_states: &ViewCallStates,
    ) -> Self {
        let active = Mutable::new(Tab::CallTree);
        let editor = Editor::new(CODE);

        Self(
            column()
                .overflow(Overflow::Hidden)
                .padding(Size3)
                .gap(Size3)
                .child(tab_bar().style(Style::Tabs).children([
                    tab(Tab::CallTree, "Call Tree", &active),
                    tab(Tab::SourceCode, "Source Code", &active),
                ]))
                .children([
                    content(
                        Tab::CallTree,
                        &active,
                        CallTree::new(
                            fn_id,
                            Actions {
                                active: active.clone(),
                                editor: editor.clone(),
                            },
                            library,
                            view_call_states,
                        ),
                    )
                    .overflow(Overflow::Auto),
                    content(Tab::SourceCode, &active, SourceView::new(&editor))
                        .flex_column()
                        .overflow(Overflow::Hidden),
                ])
                .into(),
        )
    }
}

#[derive(Clone)]
struct Actions {
    active: Mutable<Tab>,
    editor: Editor,
}

impl CallTreeActions for Actions {
    fn view_code(&self, span: SrcSpan) {
        self.editor.set_selection(span);
        self.active.set_neq(Tab::SourceCode);
    }
}

fn tab(tab: Tab, name: &str, active: &Mutable<Tab>) -> html::ButtonBuilder {
    html::button()
        .text(name)
        .active(Sig(active.signal().eq(tab)))
        .on_click({
            clone!(active);
            move |_, _| active.set(tab)
        })
}

fn content(tab: Tab, active: &Mutable<Tab>, content: impl Into<Node>) -> DivBuilder {
    div()
        .display(Sig(active.signal().map(move |active| {
            if active == tab {
                Display::Block
            } else {
                Display::None
            }
        })))
        .child(content.into())
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Tab {
    CallTree,
    SourceCode,
}
