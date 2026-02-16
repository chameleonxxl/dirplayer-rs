use std::cell::UnsafeCell;

use log::{debug, warn};

use crate::director::lingo::datum::Datum;

use super::{
    datum_ref::{DatumId, DatumRef},
    script::{ScriptInstance, ScriptInstanceId},
    script_ref::ScriptInstanceRef,
    ScriptError,
};

const ARENA_CHUNK_SIZE: usize = 4096;

pub struct Arena<T> {
    chunks: Vec<Box<[Option<T>]>>,
    free_list: Vec<usize>,
    count: usize,
    next_slot: usize,
}

impl<T> Arena<T> {
    pub fn new() -> Self {
        Arena {
            chunks: Vec::new(),
            free_list: Vec::new(),
            count: 0,
            next_slot: 0,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let num_chunks = (capacity + ARENA_CHUNK_SIZE - 1) / ARENA_CHUNK_SIZE;
        let mut chunks = Vec::with_capacity(num_chunks);
        for _ in 0..num_chunks {
            chunks.push(Self::new_chunk());
        }
        Arena {
            chunks,
            free_list: Vec::with_capacity(capacity),
            count: 0,
            next_slot: 0,
        }
    }

    fn new_chunk() -> Box<[Option<T>]> {
        let mut chunk = Vec::with_capacity(ARENA_CHUNK_SIZE);
        chunk.resize_with(ARENA_CHUNK_SIZE, || None);
        chunk.into_boxed_slice()
    }

    fn ensure_chunk(&mut self, chunk_idx: usize) {
        while self.chunks.len() <= chunk_idx {
            self.chunks.push(Self::new_chunk());
        }
    }

    pub fn alloc(&mut self, value: T) -> usize {
        self.count += 1;
        if let Some(idx) = self.free_list.pop() {
            self.chunks[idx / ARENA_CHUNK_SIZE][idx % ARENA_CHUNK_SIZE] = Some(value);
            idx + 1
        } else {
            let idx = self.next_slot;
            self.ensure_chunk(idx / ARENA_CHUNK_SIZE);
            self.chunks[idx / ARENA_CHUNK_SIZE][idx % ARENA_CHUNK_SIZE] = Some(value);
            self.next_slot += 1;
            idx + 1
        }
    }

    pub fn insert_at(&mut self, id: usize, value: T) {
        let idx = id - 1;
        self.ensure_chunk(idx / ARENA_CHUNK_SIZE);
        let chunk_idx = idx / ARENA_CHUNK_SIZE;
        let slot_idx = idx % ARENA_CHUNK_SIZE;
        // Use take() to safely drop the old value (if any) before inserting
        let was_empty = self.chunks[chunk_idx][slot_idx].take().is_none();
        self.chunks[chunk_idx][slot_idx] = Some(value);
        if was_empty {
            self.count += 1;
        }
        if idx >= self.next_slot {
            self.next_slot = idx + 1;
        }
    }

    pub fn remove(&mut self, id: usize) -> Option<T> {
        if id == 0 {
            return None;
        }
        let idx = id - 1;
        let chunk_idx = idx / ARENA_CHUNK_SIZE;
        if chunk_idx < self.chunks.len() {
            let slot_idx = idx % ARENA_CHUNK_SIZE;
            if let Some(value) = self.chunks[chunk_idx][slot_idx].take() {
                self.free_list.push(idx);
                self.count -= 1;
                Some(value)
            } else {
                None
            }
        } else {
            None
        }
    }

    #[inline]
    pub fn get(&self, id: usize) -> Option<&T> {
        if id == 0 {
            return None;
        }
        let idx = id - 1;
        let chunk_idx = idx / ARENA_CHUNK_SIZE;
        if chunk_idx < self.chunks.len() {
            self.chunks[chunk_idx][idx % ARENA_CHUNK_SIZE].as_ref()
        } else {
            None
        }
    }

    #[inline]
    pub fn get_mut(&mut self, id: usize) -> Option<&mut T> {
        if id == 0 {
            return None;
        }
        let idx = id - 1;
        let chunk_idx = idx / ARENA_CHUNK_SIZE;
        if chunk_idx < self.chunks.len() {
            self.chunks[chunk_idx][idx % ARENA_CHUNK_SIZE].as_mut()
        } else {
            None
        }
    }

    #[inline]
    pub fn contains(&self, id: usize) -> bool {
        if id == 0 {
            return false;
        }
        let idx = id - 1;
        let chunk_idx = idx / ARENA_CHUNK_SIZE;
        chunk_idx < self.chunks.len()
            && self.chunks[chunk_idx][idx % ARENA_CHUNK_SIZE].is_some()
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn clear(&mut self) {
        self.chunks.clear();
        self.free_list.clear();
        self.count = 0;
        self.next_slot = 0;
    }

    pub fn clear_individually_reverse(&mut self) {
        for chunk_idx in (0..self.chunks.len()).rev() {
            for slot_idx in (0..ARENA_CHUNK_SIZE).rev() {
                // Use take() so the slot is set to None BEFORE the value is
                // dropped. This ensures re-entrant contains() checks during
                // drop cascades correctly see the slot as empty.
                drop(self.chunks[chunk_idx][slot_idx].take());
            }
        }
        self.free_list.clear();
        self.count = 0;
        self.next_slot = 0;
    }

    pub fn clear_individually(&mut self) {
        for chunk_idx in 0..self.chunks.len() {
            for slot_idx in 0..ARENA_CHUNK_SIZE {
                drop(self.chunks[chunk_idx][slot_idx].take());
            }
        }
        self.free_list.clear();
        self.count = 0;
        self.next_slot = 0;
    }
}

pub struct DatumRefEntry {
    pub id: DatumId,
    pub ref_count: UnsafeCell<u32>,
    pub datum: Datum,
}

pub struct ScriptInstanceRefEntry {
    pub id: ScriptInstanceId,
    pub ref_count: UnsafeCell<u32>,
    pub script_instance: ScriptInstance,
}

pub trait ResetableAllocator {
    fn reset(&mut self);
}

pub trait DatumAllocatorTrait {
    fn alloc_datum(&mut self, datum: Datum) -> Result<DatumRef, ScriptError>;
    fn get_datum(&self, id: &DatumRef) -> &Datum;
    fn get_datum_mut(&mut self, id: &DatumRef) -> &mut Datum;
    fn on_datum_ref_dropped(&mut self, id: DatumId);
}

pub trait ScriptInstanceAllocatorTrait {
    fn alloc_script_instance(&mut self, script_instance: ScriptInstance) -> ScriptInstanceRef;
    fn get_script_instance(&self, instance_ref: &ScriptInstanceRef) -> &ScriptInstance;
    fn get_script_instance_opt(&self, instance_ref: &ScriptInstanceRef) -> Option<&ScriptInstance>;
    fn get_script_instance_mut(&mut self, instance_ref: &ScriptInstanceRef) -> &mut ScriptInstance;
    fn on_script_instance_ref_dropped(&mut self, id: ScriptInstanceId);
}

pub struct DatumAllocator {
    pub datums: Arena<DatumRefEntry>,
    pub script_instances: Arena<ScriptInstanceRefEntry>,
    script_instance_counter: ScriptInstanceId,
    void_datum: Datum,
}

const MAX_SCRIPT_INSTANCE_ID: ScriptInstanceId = 0xFFFFFF;

impl DatumAllocator {
    pub fn default() -> Self {
        DatumAllocator {
            datums: Arena::with_capacity(4096),
            script_instances: Arena::new(),
            script_instance_counter: 1,
            void_datum: Datum::Void,
        }
    }

    pub fn contains_datum(&self, id: DatumId) -> bool {
        self.datums.contains(id)
    }

    pub fn get_free_script_instance_id(&self) -> ScriptInstanceId {
        if self.script_instance_count() >= MAX_SCRIPT_INSTANCE_ID as usize {
            panic!("Script instance limit reached");
        }
        if !self.script_instances.contains(self.script_instance_counter as usize) {
            self.script_instance_counter
        } else if self.script_instance_counter + 1 < MAX_SCRIPT_INSTANCE_ID
            && !self
                .script_instances
                .contains((self.script_instance_counter + 1) as usize)
        {
            self.script_instance_counter + 1
        } else {
            warn!("Script instance id overflow. Searching for free id...");
            let first_free_id = (1..MAX_SCRIPT_INSTANCE_ID)
                .find(|id| !self.script_instances.contains(*id as usize));
            if let Some(id) = first_free_id {
                id
            } else {
                panic!("Failed to find free script instance id");
            }
        }
    }

    pub fn script_instance_count(&self) -> usize {
        self.script_instances.len()
    }

    pub fn datum_count(&self) -> usize {
        self.datums.len()
    }

    fn dealloc_datum(&mut self, id: DatumId) {
        self.datums.remove(id);
    }

    fn dealloc_script_instance(&mut self, id: ScriptInstanceId) {
        self.script_instances.remove(id as usize);
    }

    pub fn get_datum_ref(&self, id: DatumId) -> Option<DatumRef> {
        if let Some(entry) = self.datums.get(id) {
            Some(DatumRef::from_id(id, entry.ref_count.get()))
        } else {
            None
        }
    }

    pub fn get_script_instance_ref(&self, id: ScriptInstanceId) -> Option<ScriptInstanceRef> {
        if let Some(entry) = self.script_instances.get(id as usize) {
            Some(ScriptInstanceRef::from_id(id, entry.ref_count.get()))
        } else {
            None
        }
    }

    pub fn get_script_instance_entry(
        &self,
        id: ScriptInstanceId,
    ) -> Option<&ScriptInstanceRefEntry> {
        self.script_instances.get(id as usize)
    }

    pub fn get_script_instance_entry_mut(
        &mut self,
        id: ScriptInstanceId,
    ) -> Option<&mut ScriptInstanceRefEntry> {
        self.script_instances.get_mut(id as usize)
    }
}

impl DatumAllocatorTrait for DatumAllocator {
    fn alloc_datum(&mut self, datum: Datum) -> Result<DatumRef, ScriptError> {
        if datum.is_void() {
            return Ok(DatumRef::Void);
        }

        let entry = DatumRefEntry {
            id: 0,
            ref_count: UnsafeCell::new(0),
            datum,
        };
        let id = self.datums.alloc(entry);
        let entry = self.datums.get_mut(id).unwrap();
        entry.id = id;
        let ref_count_ptr = entry.ref_count.get();
        Ok(DatumRef::from_id(id, ref_count_ptr))
    }

    fn get_datum(&self, id: &DatumRef) -> &Datum {
        match id {
            DatumRef::Ref(id, ..) => {
                let entry = self.datums.get(*id).unwrap();
                &entry.datum
            }
            DatumRef::Void => &Datum::Void,
        }
    }

    fn get_datum_mut(&mut self, id: &DatumRef) -> &mut Datum {
        match id {
            DatumRef::Ref(id, ..) => {
                let entry = self.datums.get_mut(*id).unwrap();
                &mut entry.datum
            }
            DatumRef::Void => &mut self.void_datum,
        }
    }

    fn on_datum_ref_dropped(&mut self, id: DatumId) {
        self.dealloc_datum(id);
    }
}

impl ScriptInstanceAllocatorTrait for DatumAllocator {
    fn alloc_script_instance(&mut self, script_instance: ScriptInstance) -> ScriptInstanceRef {
        let id = script_instance.instance_id;
        self.script_instance_counter += 1;
        self.script_instances.insert_at(
            id as usize,
            ScriptInstanceRefEntry {
                id,
                ref_count: UnsafeCell::new(0),
                script_instance,
            },
        );
        let ref_count_ptr = self
            .script_instances
            .get(id as usize)
            .unwrap()
            .ref_count
            .get();
        ScriptInstanceRef::from_id(id, ref_count_ptr)
    }

    fn get_script_instance(&self, instance_ref: &ScriptInstanceRef) -> &ScriptInstance {
        &self
            .script_instances
            .get(instance_ref.id() as usize)
            .unwrap()
            .script_instance
    }

    fn get_script_instance_opt(
        &self,
        instance_ref: &ScriptInstanceRef,
    ) -> Option<&ScriptInstance> {
        self.script_instances
            .get(instance_ref.id() as usize)
            .map(|entry| &entry.script_instance)
    }

    fn get_script_instance_mut(
        &mut self,
        instance_ref: &ScriptInstanceRef,
    ) -> &mut ScriptInstance {
        &mut self
            .script_instances
            .get_mut(instance_ref.id() as usize)
            .unwrap()
            .script_instance
    }

    fn on_script_instance_ref_dropped(&mut self, id: ScriptInstanceId) {
        self.dealloc_script_instance(id);
    }
}

impl ResetableAllocator for DatumAllocator {
    fn reset(&mut self) {
        // Remove entries individually to ensure proper Drop cleanup.
        // Datum Drop impls may reference other datums, so reverse order
        // helps ensure dependents are dropped before their dependencies.
        debug!("Removing all datums");
        self.datums.clear_individually_reverse();

        debug!("Removing all script instances");
        self.script_instances.clear_individually();

        self.script_instance_counter = 1;
    }
}
