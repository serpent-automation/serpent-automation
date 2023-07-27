use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use futures::Future;
use futures_signals::signal::{Mutable, ReadOnlyMutable};
use serpent_automation_executor::{
    library::{FunctionId, Library},
    run::{CallStack, NestedBlock, RunState, StackFrame, ThreadRunState},
    syntax_tree::{self, ElseClause, LinkedBody, SrcSpan},
};
use tokio::sync::mpsc;

use crate::{
    is_expandable,
    tree::{Expandable, TreeNode},
};

pub struct CallTree {
    span: Option<SrcSpan>,
    name: String,
    run_state: Mutable<RunState>,
    body: TreeNode<Expandable<Body>>,
    run_state_map: RunStateMap,
}

#[derive(Clone)]
struct RunStateMap {
    thread_run_state: Rc<RefCell<ThreadRunState>>,
    run_state_map: Rc<RefCell<BTreeMap<CallStack, Mutable<RunState>>>>,
}

impl RunStateMap {
    pub fn new() -> Self {
        Self {
            thread_run_state: Rc::new(RefCell::new(ThreadRunState::new())),
            run_state_map: Rc::new(RefCell::new(BTreeMap::new())),
        }
    }

    pub fn update_run_state(&self, call_stack: CallStack, new_run_state: RunState) {
        self.run_state_map
            .borrow_mut()
            .entry(call_stack)
            .and_modify(|run_state| run_state.set(new_run_state));
    }

    pub fn insert(&self, call_stack: CallStack) -> Mutable<RunState> {
        let run_state = Mutable::new(self.thread_run_state.borrow().run_state(&call_stack));
        self.run_state_map
            .borrow_mut()
            .insert(call_stack, run_state.clone());
        run_state
    }
}

impl CallTree {
    pub fn root(fn_id: FunctionId, library: &Rc<Library>) -> Self {
        let f = library.lookup(fn_id);

        let mut call_stack = CallStack::new();
        call_stack.push(StackFrame::Call(fn_id));
        let run_state_map = RunStateMap::new();
        let run_state = run_state_map.insert(call_stack.clone());

        Self {
            span: f.span(),
            name: f.name().to_string(),
            run_state,
            body: Body::from_linked_body(&run_state_map, call_stack, library, f.body()),
            run_state_map,
        }
    }

    pub fn span(&self) -> Option<SrcSpan> {
        self.span
    }

    pub fn run_state(&self) -> ReadOnlyMutable<RunState> {
        self.run_state.read_only()
    }

    pub fn update_run_state(
        &self,
        mut run_state_updates: mpsc::Receiver<(CallStack, RunState)>,
    ) -> impl Future<Output = ()> + 'static {
        let run_state_map = self.run_state_map.clone();

        async move {
            while let Some((call_stack, new_run_state)) = run_state_updates.recv().await {
                run_state_map.update_run_state(call_stack, new_run_state);
            }
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn body(&self) -> &TreeNode<Expandable<Body>> {
        &self.body
    }
}

#[derive(Clone)]
pub struct Body(Rc<Vec<Statement>>);

impl Body {
    fn from_linked_body(
        run_state_map: &RunStateMap,
        call_stack: CallStack,
        library: &Rc<Library>,
        body: &syntax_tree::LinkedBody,
    ) -> TreeNode<Expandable<Self>> {
        match body {
            LinkedBody::Local(body) if is_expandable(body) => {
                let body = body.clone();

                TreeNode::Internal(Expandable::new({
                    let run_state_map = run_state_map.clone();
                    let library = library.clone();

                    move || Self::from_body(&run_state_map, call_stack, &library, &body)
                }))
            }
            LinkedBody::Python | LinkedBody::Local(_) => TreeNode::Leaf,
        }
    }

    fn from_body(
        run_state_map: &RunStateMap,
        call_stack: CallStack,
        library: &Rc<Library>,
        body: &syntax_tree::Body<FunctionId>,
    ) -> Self {
        let mut stmts = Vec::new();

        for (index, stmt) in body.iter().enumerate() {
            let call_stack = call_stack.push_cloned(StackFrame::Statement(index));

            match stmt {
                syntax_tree::Statement::Pass => (),
                syntax_tree::Statement::Expression(expr) => stmts.extend(
                    Call::from_expression(run_state_map, call_stack, library, expr)
                        .into_iter()
                        .map(Statement::Call),
                ),
                syntax_tree::Statement::If {
                    if_span,
                    condition,
                    then_block,
                    else_block,
                } => stmts.push(Statement::If(If::new(
                    run_state_map,
                    call_stack,
                    library,
                    *if_span,
                    condition,
                    then_block,
                    else_block,
                ))),
            }
        }

        Self(Rc::new(stmts))
    }

    pub fn iter(&self) -> impl Iterator<Item = &'_ Statement> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub enum Statement {
    Call(Call),
    If(If),
}

#[derive(Clone)]
pub struct Call {
    span: SrcSpan,
    name: String,
    run_state: Mutable<RunState>,
    body: TreeNode<Expandable<Body>>,
}

impl Call {
    fn new(
        run_state_map: &RunStateMap,
        call_stack: CallStack,
        library: &Rc<Library>,
        span: SrcSpan,
        name: FunctionId,
    ) -> Self {
        let function = &library.lookup(name);

        Self {
            span,
            name: function.name().to_string(),
            run_state: run_state_map.insert(call_stack.clone()),
            body: Body::from_linked_body(run_state_map, call_stack, library, function.body()),
        }
    }

    fn from_expression(
        run_state_map: &RunStateMap,
        call_stack: CallStack,
        library: &Rc<Library>,
        expr: &syntax_tree::Expression<FunctionId>,
    ) -> Vec<Call> {
        match expr {
            syntax_tree::Expression::Literal(_) | syntax_tree::Expression::Variable { .. } => {
                Vec::new()
            }
            syntax_tree::Expression::Call { span, name, args } => {
                let mut calls = Vec::new();

                for (index, arg) in args.iter().enumerate() {
                    calls.extend(Self::from_expression(
                        run_state_map,
                        call_stack.push_cloned(StackFrame::Argument(index)),
                        library,
                        arg,
                    ));
                }

                calls.push(Self::new(
                    run_state_map,
                    call_stack.push_cloned(StackFrame::Call(*name)),
                    library,
                    *span,
                    *name,
                ));
                calls
            }
        }
    }

    pub fn span(&self) -> SrcSpan {
        self.span
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn run_state(&self) -> ReadOnlyMutable<RunState> {
        self.run_state.read_only()
    }

    pub fn body(&self) -> &TreeNode<Expandable<Body>> {
        &self.body
    }
}

pub struct If {
    span: SrcSpan,
    run_state: Mutable<RunState>,
    condition: TreeNode<Expandable<Vec<Call>>>,
    then_block: Body,
    else_block: Option<Else>,
}

impl If {
    fn new(
        run_state_map: &RunStateMap,
        mut call_stack: CallStack,
        library: &Rc<Library>,
        span: SrcSpan,
        condition: &syntax_tree::Expression<FunctionId>,
        then_block: &syntax_tree::Body<FunctionId>,
        else_block: &Option<syntax_tree::ElseClause<FunctionId>>,
    ) -> Self {
        // TODO: Tidy this
        call_stack.push(StackFrame::NestedBlock(0, NestedBlock::Predicate));

        let calls = Call::from_expression(run_state_map, call_stack.clone(), library, condition);
        let run_state = run_state_map.insert(call_stack.clone());
        call_stack.pop();
        let then_block = Body::from_body(
            run_state_map,
            call_stack.push_cloned(StackFrame::NestedBlock(0, NestedBlock::Body)),
            library,
            then_block,
        );

        Self {
            span,
            run_state,
            condition: if calls.is_empty() {
                TreeNode::Leaf
            } else {
                TreeNode::Internal(Expandable::new(|| calls))
            },
            then_block,
            else_block: else_block
                .as_ref()
                .map(|else_block| Else::new(1, run_state_map, call_stack, library, else_block)),
        }
    }

    pub fn span(&self) -> SrcSpan {
        self.span
    }

    pub fn run_state(&self) -> ReadOnlyMutable<RunState> {
        self.run_state.read_only()
    }

    pub fn condition(&self) -> &TreeNode<Expandable<Vec<Call>>> {
        &self.condition
    }

    pub fn then_block(&self) -> &Body {
        &self.then_block
    }

    pub fn else_block(&self) -> &Option<Else> {
        &self.else_block
    }
}

pub struct Else {
    span: SrcSpan,
    run_state: Mutable<RunState>,
    body: Body,
}

impl Else {
    fn new(
        block_index: usize,
        run_state_map: &RunStateMap,
        mut call_stack: CallStack,
        library: &Rc<Library>,
        else_block: &ElseClause<FunctionId>,
    ) -> Self {
        let run_state = run_state_map.insert(
            call_stack.push_cloned(StackFrame::NestedBlock(block_index, NestedBlock::Predicate)),
        );

        call_stack.push(StackFrame::NestedBlock(block_index, NestedBlock::Body));

        Self {
            span: else_block.span(),
            run_state,
            body: Body::from_body(run_state_map, call_stack, library, else_block.body()),
        }
    }

    pub fn run_state(&self) -> ReadOnlyMutable<RunState> {
        self.run_state.read_only()
    }

    pub fn span(&self) -> SrcSpan {
        self.span
    }

    pub fn body(&self) -> &Body {
        &self.body
    }
}
