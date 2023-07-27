use std::{
    cmp::Ordering,
    collections::HashSet,
    pin::pin,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use futures::{executor::block_on, Stream};
use serde::{Deserialize, Serialize};
use slotmap::{new_key_type, SlotMap};
// TODO: Split this out into a separate crate (or put it in the server crate)?
#[cfg(not(target_arch = "wasm32"))]
use tokio::spawn;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::StreamExt;

use crate::library::FunctionId;

// The order of the enum variants is important, as we rely on later call stacks
// to be greater than earlier ones.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum StackFrame {
    Statement(usize),
    Argument(usize),
    Call(FunctionId),
    NestedBlock(usize, NestedBlock),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum NestedBlock {
    Predicate,
    Body,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct CallStack(Vec<StackFrame>);

impl Ord for CallStack {
    fn cmp(&self, other: &Self) -> Ordering {
        for (i, j) in self.0.iter().zip(other.0.iter()) {
            let cmp = i.cmp(j);

            if cmp != Ordering::Equal {
                return cmp;
            }
        }

        other.0.len().cmp(&self.0.len())
    }
}

impl PartialOrd for CallStack {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl CallStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn starts_with(&self, other: &Self) -> bool {
        self.0.starts_with(&other.0)
    }

    // TODO: We need a "slice" for CallStacks
    pub fn parent(&self) -> Option<CallStack> {
        let mut parent = self.0.clone();
        parent.pop().map(|_| Self(parent))
    }

    pub fn push(&mut self, item: StackFrame) {
        self.0.push(item)
    }

    pub fn push_cloned(&self, item: StackFrame) -> Self {
        let mut clone = self.clone();
        clone.push(item);
        clone
    }

    pub fn pop(&mut self) {
        self.0.pop();
    }
}

#[derive(Clone)]
pub struct ThreadRunState(Arc<RwLock<SharedThreadRunState>>);

// TODO: Split this out into a separate crate (or put it in the server crate)?
#[cfg(not(target_arch = "wasm32"))]
impl Default for ThreadRunState {
    fn default() -> Self {
        let (update_sender, update_receiver) = mpsc::channel(1000);
        let updater = ThreadRunStateUpdater {
            clients: Default::default(),
        };

        spawn({
            let mut updater = updater.clone();
            async move { updater.update_clients(update_receiver).await }
        });

        Self(Arc::new(RwLock::new(SharedThreadRunState {
            history: Vec::new(),
            last_completed: None,
            current: CallStack::new(),
            updater,
            update_sender,
        })))
    }
}

struct SharedThreadRunState {
    history: Vec<(CallStack, RunState)>,
    last_completed: Option<CallStack>,
    current: CallStack,
    updater: ThreadRunStateUpdater,
    update_sender: mpsc::Sender<(CallStack, RunState)>,
}

impl SharedThreadRunState {
    fn update(&self, call_stack: CallStack, run_state: RunState) {
        block_on(self.update_sender.send((call_stack, run_state))).unwrap();
    }
}

impl ThreadRunState {
    pub fn run_state(&self, stack: &CallStack) -> RunState {
        let data = self.read();

        if data.current.starts_with(stack) {
            return RunState::Running;
        }

        if Some(stack) > data.last_completed.as_ref() {
            return RunState::NotRun;
        }

        let insert_index = match data
            .history
            .binary_search_by_key(&stack, |(call_stack, _)| call_stack)
        {
            Ok(match_index) => return data.history[match_index].1,
            Err(insert_index) => insert_index,
        };

        if insert_index == 0 {
            return RunState::NotRun;
        }

        let run_state = data.history[insert_index - 1].1;

        if run_state == RunState::PredicateSuccessful(false) {
            RunState::NotRun
        } else {
            run_state
        }
    }

    pub fn push(&self, item: StackFrame) {
        let mut data = self.write();
        data.current.push(item);
        data.update(data.current.clone(), RunState::Running);
    }

    pub fn pop_success(&self) {
        self.pop(RunState::Successful);
    }

    pub fn pop_failed(&self) {
        self.pop(RunState::Failed);
    }

    pub fn pop_predicate_success(&self, result: bool) {
        self.pop(RunState::PredicateSuccessful(result));
    }

    fn pop(&self, run_state: RunState) {
        let mut data = self.write();
        let store_run_state = if let Some((last, last_run_state)) = data.history.last() {
            assert!(last < &data.current);

            // We always need to store `PredicateSuccessful(false)` as that is used to
            // indicate the start of a gap of `NotRun`.
            *last_run_state == RunState::PredicateSuccessful(false) || *last_run_state != run_state
        } else {
            true
        };

        if store_run_state {
            let current = data.current.clone();
            data.history.push((current, run_state));
        }

        data.last_completed = Some(data.current.clone());
        data.update(data.current.clone(), run_state);
        data.current.pop();
    }

    // TODO: Split this out into a separate crate (or put it in the server crate)?
    #[cfg(not(target_arch = "wasm32"))]
    pub fn subscribe(
        &self,
        open_nodes: impl Stream<Item = CallStack> + Send + 'static,
    ) -> broadcast::Receiver<(CallStack, RunState)> {
        self.write().updater.subscribe(open_nodes)
    }

    fn read(&self) -> RwLockReadGuard<'_, SharedThreadRunState> {
        self.0.read().unwrap()
    }

    fn write(&self) -> RwLockWriteGuard<'_, SharedThreadRunState> {
        self.0.write().unwrap()
    }
}

new_key_type! {struct ClientId; }

#[derive(Clone)]
pub struct ThreadRunStateUpdater {
    clients: Arc<RwLock<SlotMap<ClientId, Arc<Client>>>>,
}

impl ThreadRunStateUpdater {
    pub async fn update_clients(
        &mut self,
        mut update_receiver: mpsc::Receiver<(CallStack, RunState)>,
    ) {
        // TODO(next): Update receiver should receive new subscriptions and updates, so
        // we don't have any ordering problems. It should put new subscriptions
        // in the map and send the initial values.
        // TODO(next): Send run state for newly opened nodes.

        while let Some((call_stack, run_state)) = update_receiver.recv().await {
            if let Some(parent) = call_stack.parent() {
                for client in self.clients.read().unwrap().values() {
                    if client.open_nodes.read().unwrap().contains(&parent) {
                        client
                            .view_nodes
                            .send((call_stack.clone(), run_state))
                            .unwrap();
                    }
                }
            }
        }
    }

    // TODO: Split this out into a separate crate (or put it in the server crate)?
    #[cfg(not(target_arch = "wasm32"))]
    pub fn subscribe(
        &mut self,
        open_nodes: impl Stream<Item = CallStack> + Send + 'static,
    ) -> broadcast::Receiver<(CallStack, RunState)> {
        // TODO: Channel bounds
        let (run_state_sender, run_state_receiver) = broadcast::channel(1000);
        let client = Arc::new(Client::new(run_state_sender));
        let id = { self.clients.write().unwrap().insert(client.clone()) };

        spawn({
            // TODO: Use clone! from silkenweb
            let client = client.clone();
            let clients = self.clients.clone();

            async move {
                let mut open_nodes = pin!(open_nodes);

                while let Some(node) = open_nodes.next().await {
                    // TODO(next): Don't write to open nodes here. Just send a message to
                    // `update_clients` saying we're interested.
                    client.open_nodes.write().unwrap().insert(node);
                }

                clients.write().unwrap().remove(id);
            }
        });

        run_state_receiver
    }
}

struct Client {
    open_nodes: RwLock<HashSet<CallStack>>,
    view_nodes: broadcast::Sender<(CallStack, RunState)>,
}

impl Client {
    fn new(view_nodes: broadcast::Sender<(CallStack, RunState)>) -> Self {
        Self {
            open_nodes: RwLock::new(HashSet::new()),
            view_nodes,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum RunState {
    NotRun,
    Running,
    Successful,
    PredicateSuccessful(bool),
    Failed,
}
