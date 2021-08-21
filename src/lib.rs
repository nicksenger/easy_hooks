use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use std::rc::Rc;

use anymap::any::Any;
use slotmap::{DefaultKey, DenseSlotMap, Key, SecondaryMap};

pub use topo::{call_in_slot, nested, root};

thread_local! {
    static CONTEXT_ID: Cell<u64> = Cell::new(0);
    static STORE: RefCell<Store> = RefCell::new(Store::new());
}

/// Clears any state which was not accessed since the last sweep.
pub fn sweep() {
    STORE.with(|store_refcell| {
        store_refcell.borrow_mut().sweep();
    });
}

/// Creates new local state with the given `data_fn`, or provides a handle to the local state
/// if it already exists.
pub fn use_state<T: 'static, F: FnOnce() -> T>(data_fn: F) -> LocalState<T> {
    let id = Id::new();

    if !state_exists_for_id::<T>(id) {
        set_state_with_id::<T>(data_fn(), id);
    } else if !state_marked_with_id::<T>(id) {
        mark_state_with_id::<T>(id);
    }

    LocalState::new(id)
}

/// Creates a new context with the provided type.
pub fn create_context<T: 'static>() -> Context<T> {
    let context_id = CONTEXT_ID.with(|id| id.get());
    let id = Id::in_context(context_id);
    CONTEXT_ID.with(|id| id.set(id.get() + 1));

    Context::new(id)
}

pub struct Context<T> {
    id: Id,
    _phantom: PhantomData<T>,
}

impl<T: 'static> Context<T> {
    fn new(id: Id) -> Self {
        Self {
            id,
            _phantom: PhantomData,
        }
    }

    pub fn get<U, F: FnOnce(&Rc<T>) -> U>(&self, f: F) -> U {
        if !state_exists_for_id::<Rc<T>>(self.id) {
            panic!("Attempted to get data from a context which was never set.")
        } else if !state_marked_with_id::<Rc<T>>(self.id) {
            mark_state_with_id::<Rc<T>>(self.id);
        }

        read_state_with_id(self.id, f)
    }

    pub fn set(&self, data: T) {
        set_state_with_id(Rc::new(data), self.id)
    }
}

fn set_state_with_id<T: 'static>(data: T, current_id: Id) {
    STORE.with(|store_refcell| {
        store_refcell
            .borrow_mut()
            .set_state_with_id::<T>(data, &current_id)
    });
}

fn mark_state_with_id<T: 'static>(current_id: Id) {
    STORE.with(|store_refcell| {
        store_refcell
            .borrow_mut()
            .mark_state_with_id::<T>(&current_id)
    });
}

fn state_exists_for_id<T: 'static>(id: Id) -> bool {
    STORE.with(|store_refcell| store_refcell.borrow().state_exists_with_id::<T>(id))
}

fn state_marked_with_id<T: 'static>(id: Id) -> bool {
    STORE.with(|store_refcell| store_refcell.borrow().state_marked_with_id::<T>(id))
}

fn remove_state_with_id<T: 'static>(id: Id) -> Option<T> {
    STORE.with(|store_refcell| store_refcell.borrow_mut().remove_state_with_id::<T>(&id))
}

fn update_state_with_id<T: 'static, F: FnOnce(&mut T) -> U, U>(id: Id, func: F) -> U {
    let mut item = remove_state_with_id::<T>(id).expect("State does not exist.");
    let updated = func(&mut item);
    set_state_with_id(item, id);
    updated
}

fn read_state_with_id<T: 'static, F: FnOnce(&T) -> R, R>(id: Id, func: F) -> R {
    let item = remove_state_with_id::<T>(id).expect("State does not exist.");
    let read = func(&item);
    set_state_with_id(item, id);
    read
}

pub struct LocalState<T> {
    id: Id,
    _phantom_data: PhantomData<T>,
}

impl<T> std::fmt::Debug for LocalState<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({:#?})", self.id)
    }
}

impl<T> Copy for LocalState<T> {}
impl<T> Clone for LocalState<T> {
    fn clone(&self) -> LocalState<T> {
        LocalState::<T> {
            id: self.id,
            _phantom_data: PhantomData::<T>,
        }
    }
}

impl<T> LocalState<T>
where
    T: 'static,
{
    fn new(id: Id) -> LocalState<T> {
        LocalState {
            id,
            _phantom_data: PhantomData,
        }
    }

    pub fn set<F: FnOnce(&mut T) -> U, U>(self, func: F) -> U {
        update_state_with_id(self.id, func)
    }

    pub fn get<F: FnOnce(&T) -> R, R>(self, func: F) -> R {
        read_state_with_id(self.id, func)
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Hash)]
struct Id {
    id: topo::CallId,
}

impl Id {
    #[topo::nested]
    fn new() -> Self {
        Self {
            id: topo::CallId::current(),
        }
    }

    fn in_context(context_id: u64) -> Self {
        topo::root(|| topo::call_in_slot(&context_id, Self::new))
    }
}

#[derive(Clone, Copy)]
enum Mode {
    A,
    B,
}

impl Mode {
    fn reverse(&self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

struct Store {
    data_a: anymap::Map<dyn Any>,
    data_b: anymap::Map<dyn Any>,
    mode: Mode,
    keys_by_id: HashMap<Id, DefaultKey>,
    ids: DenseSlotMap<DefaultKey, Id>,
}

impl Store {
    pub fn new() -> Store {
        Store {
            data_a: anymap::Map::new(),
            data_b: anymap::Map::new(),
            ids: DenseSlotMap::new(),
            keys_by_id: HashMap::new(),
            mode: Mode::A,
        }
    }

    pub fn sweep(&mut self) {
        match self.mode {
            Mode::A => {
                self.data_b.clear();
                self.mode = Mode::B;
            }
            Mode::B => {
                self.data_a.clear();
                self.mode = Mode::A;
            }
        }
    }

    pub fn state_exists_with_id<T: 'static>(&self, id: Id) -> bool {
        self.state_exists::<T>(self.mode, id) || self.state_exists::<T>(self.mode.reverse(), id)
    }

    pub fn state_marked_with_id<T: 'static>(&self, id: Id) -> bool {
        self.state_exists::<T>(self.mode, id) || !self.state_exists::<T>(self.mode.reverse(), id)
    }

    pub fn remove_state_with_id<T: 'static>(&mut self, current_id: &Id) -> Option<T> {
        //unwrap or default to keep borrow checker happy
        let key = self.keys_by_id.get(current_id).copied().unwrap_or_default();

        if key.is_null() {
            None
        } else {
            self.get_mut_secondarymap::<T>(self.mode).remove(key)
        }
    }

    pub fn set_state_with_id<T: 'static>(&mut self, data: T, current_id: &Id) {
        let key = self.keys_by_id.get(current_id).copied().unwrap_or_default();

        if key.is_null() {
            let key = self.ids.insert(*current_id);
            self.keys_by_id.insert(*current_id, key);
            self.get_mut_secondarymap::<T>(self.mode).insert(key, data);
        } else {
            self.get_mut_secondarymap::<T>(self.mode).insert(key, data);
        }
    }

    pub fn mark_state_with_id<T: 'static>(&mut self, current_id: &Id) {
        let key = self.keys_by_id.get(current_id).copied().unwrap_or_default();

        if !key.is_null() {
            let data = self
                .get_mut_secondarymap::<T>(self.mode.reverse())
                .remove(key)
                .unwrap();
            self.get_mut_secondarymap(self.mode).insert(key, data);
        }
    }

    fn state_exists<T: 'static>(&self, mode: Mode, id: Id) -> bool {
        match (self.keys_by_id.get(&id), self.get_secondarymap::<T>(mode)) {
            (Some(existing_key), Some(existing_secondary_map)) => {
                existing_secondary_map.contains_key(*existing_key)
            }
            (_, _) => false,
        }
    }

    fn get_secondarymap<T: 'static>(&self, mode: Mode) -> Option<&SecondaryMap<DefaultKey, T>> {
        self.get_datamap(mode).get::<SecondaryMap<DefaultKey, T>>()
    }

    fn get_mut_secondarymap<T: 'static>(&mut self, mode: Mode) -> &mut SecondaryMap<DefaultKey, T> {
        if self
            .get_datamap_mut(mode)
            .get_mut::<SecondaryMap<DefaultKey, T>>()
            .is_some()
        {
            self.get_datamap_mut(mode)
                .get_mut::<SecondaryMap<DefaultKey, T>>()
                .unwrap()
        } else {
            self.register_secondarymap::<T>(mode);
            self.get_datamap_mut(mode)
                .get_mut::<SecondaryMap<DefaultKey, T>>()
                .unwrap()
        }
    }

    fn register_secondarymap<T: 'static>(&mut self, mode: Mode) {
        let sm: SecondaryMap<DefaultKey, T> = SecondaryMap::new();
        self.get_datamap_mut(mode).insert(sm);
    }

    fn get_datamap(&self, mode: Mode) -> &anymap::Map<dyn Any> {
        match mode {
            Mode::A => &self.data_a,
            Mode::B => &self.data_b,
        }
    }

    fn get_datamap_mut(&mut self, mode: Mode) -> &mut anymap::Map<dyn Any> {
        match mode {
            Mode::A => &mut self.data_a,
            Mode::B => &mut self.data_b,
        }
    }
}
