use std::rc::Rc;

use futures_signals::signal::{Mutable, Signal};
use once_cell::unsync::Lazy;
use serpent_automation_executor::{
    library::{FunctionId, Library},
    syntax_tree::{self, LinkedBody, SrcSpan},
};

use crate::is_expandable;

pub struct CallTree {
    name: String,
    body: Vertex<Expandable<Body>>,
}

impl CallTree {
    pub fn root(fn_id: FunctionId, library: &Rc<Library>) -> Self {
        let f = library.lookup(fn_id);
        let body = match f.body() {
            LinkedBody::Local(body) if is_expandable(body) => {
                let body = body.clone();
                Vertex::Node(Expandable::new({
                    let library = library.clone();
                    move || Body::new(&library, &body)
                }))
            }
            LinkedBody::Python | LinkedBody::Local(_) => Vertex::Leaf,
        };

        Self {
            name: f.name().to_string(),
            body,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn children(&self) -> Vertex<impl Signal<Item = Option<Body>>> {
        self.body.map(|body| body.item())
    }
}

pub type DynLazy<T> = Lazy<T, Box<dyn FnOnce() -> T>>;

#[derive(Clone)]
pub enum Vertex<Children> {
    Leaf,
    Node(Children),
}

impl<Children> Vertex<Children> {
    pub fn map<R>(&self, f: impl FnOnce(&Children) -> R) -> Vertex<R> {
        match self {
            Vertex::Leaf => Vertex::Leaf,
            Vertex::Node(children) => Vertex::Node(f(children)),
        }
    }
}

#[derive(Clone)]
pub struct Expandable<Item> {
    expanded: Mutable<bool>,
    item: Rc<DynLazy<Item>>,
}

impl<Item: Clone> Expandable<Item> {
    pub fn new(f: impl FnOnce() -> Item + 'static) -> Self {
        Self {
            expanded: Mutable::new(false),
            item: Rc::new(Lazy::new(Box::new(f))),
        }
    }

    pub fn collapse(&self) {
        self.expanded.set_neq(false)
    }

    pub fn expand(&self) {
        self.expanded.set_neq(true)
    }

    pub fn item(&self) -> impl Signal<Item = Option<Item>> {
        let item = self.item.clone();

        self.expanded
            .signal_ref(move |expanded| expanded.then(|| (*item).clone()))
    }
}

#[derive(Clone)]
pub struct Body(Rc<Vec<Statement>>);

impl Body {
    pub fn new(library: &Rc<Library>, body: &syntax_tree::Body<FunctionId>) -> Self {
        let mut stmts = Vec::new();

        for stmt in body.iter() {
            match stmt {
                syntax_tree::Statement::Pass => (),
                syntax_tree::Statement::Expression(expr) => match expr {
                    syntax_tree::Expression::Literal(_) => (),
                    syntax_tree::Expression::Variable { .. } => (),
                    syntax_tree::Expression::Call { span, name, args } => stmts.push(Statement {
                        span: *span,
                        body: StatementBody::Call(Call::new(library, *span, *name, args)),
                    }),
                },
                syntax_tree::Statement::If {
                    if_span,
                    condition,
                    then_block,
                    else_block,
                } => stmts.push(Statement {
                    span: *if_span,
                    // TODO: Implement
                    body: StatementBody::If,
                }),
            }
        }

        Self(Rc::new(stmts))
    }
}

pub struct Statement {
    span: SrcSpan,
    body: StatementBody,
}

pub enum StatementBody {
    Call(Call),
    If,
}

#[derive(Clone)]
pub struct Call {
    span: SrcSpan,
    args: Vec<Self>,
    name: String,
}

impl Call {
    fn new(
        library: &Rc<Library>,
        span: SrcSpan,
        name: FunctionId,
        args: &[syntax_tree::Expression<FunctionId>],
    ) -> Self {
        let function = &library.lookup(name);
        let name = function.name().to_string();
        let args =
        args.iter()
                .filter_map(|arg| match arg {
                    syntax_tree::Expression::Literal(_)
                    | syntax_tree::Expression::Variable { .. } => None,
                    syntax_tree::Expression::Call { span, name, args } => {
                        Some(Self::new(library, *span, *name, args))
                    }
                })
                .collect();
        
        // TODO: Extract function body into `Vertex<Expandable>`
        Self { span, name, args }
    }
}
