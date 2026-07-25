use blake2::{Blake2b512, Digest};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const BLOOM_SIZE: usize = 4096;
const NUM_HASHES: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterpartyList {
    pub tenant_id: Uuid,
    bloom_filter: Vec<u8>,
    counterparty_count: usize,
}

impl CounterpartyList {
    pub fn new(tenant_id: Uuid) -> Self {
        Self {
            tenant_id,
            bloom_filter: vec![0u8; BLOOM_SIZE],
            counterparty_count: 0,
        }
    }

    fn hash_indices(id: &Uuid) -> [usize; NUM_HASHES] {
        let bytes = id.as_bytes();
        let mut indices = [0usize; NUM_HASHES];
        for i in 0..NUM_HASHES {
            let mut hasher = Blake2b512::new();
            hasher.update(bytes);
            hasher.update(&[i as u8]);
            let hash = hasher.finalize();
            let start = (i as usize) % 8;
            let mut word = [0u8; 8];
            word.copy_from_slice(&hash[start..start + 8]);
            indices[i] = (u64::from_le_bytes(word) as usize) % (BLOOM_SIZE * 8);
        }
        indices
    }

    fn set_bit(filter: &mut [u8], index: usize) {
        let byte = index / 8;
        let bit = index % 8;
        if byte < filter.len() {
            filter[byte] |= 1 << bit;
        }
    }

    fn test_bit(filter: &[u8], index: usize) -> bool {
        let byte = index / 8;
        let bit = index % 8;
        if byte < filter.len() {
            (filter[byte] >> bit) & 1 == 1
        } else {
            false
        }
    }

    pub fn add(&mut self, counterparty_id: &Uuid) {
        let indices = Self::hash_indices(counterparty_id);
        for idx in indices {
            Self::set_bit(&mut self.bloom_filter, idx);
        }
        self.counterparty_count += 1;
    }

    pub fn contains(&self, counterparty_id: &Uuid) -> bool {
        let indices = Self::hash_indices(counterparty_id);
        indices.iter().all(|idx| Self::test_bit(&self.bloom_filter, *idx))
    }

    pub fn count(&self) -> usize {
        self.counterparty_count
    }
}

pub struct CounterpartyVisibilityStore {
    lists: DashMap<Uuid, CounterpartyList>,
}

impl CounterpartyVisibilityStore {
    pub fn new() -> Self {
        Self {
            lists: DashMap::new(),
        }
    }

    pub fn add_counterparty(&self, tenant_id: &Uuid, counterparty_id: &Uuid) -> Result<(), String> {
        if tenant_id == counterparty_id {
            return Err("cannot add self as counterparty".to_string());
        }
        let mut list = self.lists
            .entry(*tenant_id)
            .or_insert_with(|| CounterpartyList::new(*tenant_id));
        list.add(counterparty_id);
        Ok(())
    }

    pub fn accepts(&self, taker_id: &Uuid, maker_id: &Uuid) -> bool {
        let list = match self.lists.get(taker_id) {
            Some(l) => l,
            None => return true,
        };
        if list.counterparty_count == 0 {
            return true;
        }
        list.contains(maker_id)
    }

    pub fn mutual_acceptance(&self, a_id: &Uuid, b_id: &Uuid) -> bool {
        self.accepts(a_id, b_id) && self.accepts(b_id, a_id)
    }

    pub fn get_list(&self, tenant_id: &Uuid) -> Option<CounterpartyList> {
        self.lists.get(tenant_id).map(|r| r.clone())
    }

    pub fn remove_counterparty(&self, tenant_id: &Uuid, counterparty_id: &Uuid) {
        // Collect surviving tenant IDs first (avoid holding locks during iteration)
        let surviving: Vec<Uuid> = self.lists.iter()
            .filter(|e| e.tenant_id != *counterparty_id && e.tenant_id != *tenant_id)
            .map(|e| e.tenant_id)
            .collect();

        if let Some(mut list) = self.lists.get_mut(tenant_id) {
            let new_list = CounterpartyList::new(*tenant_id);
            let old = std::mem::replace(&mut *list, new_list);
            drop(list); // release write lock before rebuilding
            for id in &surviving {
                if old.contains(id) {
                    if let Some(mut l) = self.lists.get_mut(tenant_id) {
                        l.add(id);
                    }
                }
            }
        }
    }

    pub fn tenant_count(&self) -> usize {
        self.lists.len()
    }
}
