use parking_lot::Mutex;
use std::collections::VecDeque;

pub type PackedIndex = usize;

const STATE_FREE: u8 = 0;
pub const STATE_INGESTED: u8 = 1;
pub const STATE_ML_ACQUIRED: u8 = 2;
pub const STATE_ML_COMMITTED: u8 = 3;
pub const STATE_GPU_UPLOADED: u8 = 4; // video only
pub const STATE_CONSUMED: u8 = 5; // audio only

#[repr(C)]
pub struct Slot<const SIZE: usize> {
    pub payload: [u8; SIZE],
    pub generation: u32,
    pub state: u8,
    pub pts_ns: u64,
    pub seek_gen: u64,
}

impl<const SIZE: usize> Slot<SIZE> {
    pub const fn new() -> Self {
        Self {
            payload: [0u8; SIZE],
            generation: 0,
            state: STATE_FREE,
            pts_ns: 0,
            seek_gen: 0,
        }
    }
}

struct PoolState<const SIZE: usize> {
    slots: Vec<Slot<SIZE>>,
    free: VecDeque<usize>,
}

pub struct SlotPool<const SIZE: usize> {
    state: Mutex<PoolState<SIZE>>,
}

impl<const SIZE: usize> SlotPool<SIZE> {
    pub fn new(count: usize) -> Self {
        let mut slots = Vec::with_capacity(count);
        for _ in 0..count {
            slots.push(Slot::new());
        }
        let free: VecDeque<usize> = (0..count).collect();
        let state = PoolState { slots, free };
        SlotPool {
            state: Mutex::new(state),
        }
    }

    #[inline(always)]
    fn pack_index(index: usize, generation: u32) -> PackedIndex {
        ((index as PackedIndex) << 32) | (generation as PackedIndex)
    }

    #[inline(always)]
    fn unpack_index(packed: PackedIndex) -> (usize, u32) {
        let index = (packed >> 32) as usize;
        let generation = (packed & 0xFFFF_FFFF) as u32;
        (index, generation)
    }

    /// Claim a free slot. Returns packed index with current generation, sets state to INGESTED.
    pub fn try_claim(&self) -> Option<PackedIndex> {
        let mut state = self.state.lock();
        let idx = state.free.pop_front()?;
        let slot = &mut state.slots[idx];
        debug_assert_eq!(slot.state, STATE_FREE);
        slot.state = STATE_INGESTED;
        Some(Self::pack_index(idx, slot.generation))
    }

    /// Transition the slot to a new state, validating the expected current state.
    /// Returns `Ok(())` or an error string on mismatch.
    pub fn transition_state(
        &self,
        packed: PackedIndex,
        expected_current: u8,
        new_state: u8,
    ) -> Result<(), String> {
        let (idx, generation) = Self::unpack_index(packed);
        let mut state = self.state.lock();
        let slot = &mut state.slots[idx];
        if slot.generation != generation {
            return Err("Generation mismatch".to_string());
        }
        if slot.state != expected_current {
            return Err(format!(
                "State mismatch: expected {}, got {}",
                expected_current, slot.state
            ));
        }
        slot.state = new_state;
        Ok(())
    }

    /// Release an audio slot (must be in CONSUMED state). Resets to FREE and pushes to free.
    pub fn release_audio(&self, packed: PackedIndex) {
        self.release(packed, STATE_CONSUMED);
    }

    /// Release a video slot (must be in GPU_UPLOADED state). Resets to FREE.
    pub fn release_video(&self, packed: PackedIndex) {
        self.release(packed, STATE_GPU_UPLOADED);
    }

    fn release(&self, packed: PackedIndex, expected_state: u8) {
        let (idx, generation) = Self::unpack_index(packed);
        let mut state = self.state.lock();
        let slot = &mut state.slots[idx];
        debug_assert_eq!(slot.generation, generation);
        debug_assert_eq!(slot.state, expected_state);
        slot.generation = slot.generation.wrapping_add(1);
        slot.state = STATE_FREE;
        state.free.push_back(idx);
    }

    /// Execute a closure with a mutable reference to the payload.
    /// The slot must be claimed (state != FREE) and generation must match.
    pub fn with_payload_mut<F, R>(&self, packed: PackedIndex, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let (idx, generation) = Self::unpack_index(packed);
        let mut state = self.state.lock();
        let slot = &mut state.slots[idx];
        debug_assert_eq!(slot.generation, generation);
        debug_assert_ne!(slot.state, STATE_FREE);
        f(&mut slot.payload)
    }

    pub fn state(&self, packed: PackedIndex) -> u8 {
        let (idx, _) = Self::unpack_index(packed);
        let state = self.state.lock();
        state.slots[idx].state
    }

    pub fn generation(&self, packed: PackedIndex) -> u32 {
        let (idx, _) = Self::unpack_index(packed);
        let state = self.state.lock();
        state.slots[idx].generation
    }

    pub fn set_pts_ns(&self, packed: PackedIndex, pts_ns: u64) {
        let (idx, generation) = Self::unpack_index(packed);
        let mut state = self.state.lock();
        let slot = &mut state.slots[idx];
        debug_assert_eq!(slot.generation, generation);
        debug_assert_ne!(slot.state, STATE_FREE);
        slot.pts_ns = pts_ns;
    }

    pub fn get_pts_ns(&self, packed: PackedIndex) -> u64 {
        let (idx, generation) = Self::unpack_index(packed);
        let state = self.state.lock();
        let slot = &state.slots[idx];
        debug_assert_eq!(slot.generation, generation);
        debug_assert_ne!(slot.state, STATE_FREE);
        slot.pts_ns
    }

    pub fn set_seek_gen(&self, packed: PackedIndex, seek_gen: u64) {
        let (idx, generation) = Self::unpack_index(packed);
        let mut state = self.state.lock();
        let slot = &mut state.slots[idx];
        debug_assert_eq!(slot.generation, generation);
        debug_assert_ne!(slot.state, STATE_FREE);
        slot.seek_gen = seek_gen;
    }

    pub fn get_seek_gen(&self, packed: PackedIndex) -> u64 {
        let (idx, generation) = Self::unpack_index(packed);
        let state = self.state.lock();
        let slot = &state.slots[idx];
        debug_assert_eq!(slot.generation, generation);
        debug_assert_ne!(slot.state, STATE_FREE);
        slot.seek_gen
    }

    /// Discard a slot that was claimed but should be dropped (e.g., during a generation mismatch).
    /// Resets the slot to FREE with an incremented generation and pushes it back to the free list.
    pub fn discard_slot(&self, packed: PackedIndex) {
        let (idx, generation) = Self::unpack_index(packed);
        let mut state = self.state.lock();
        let slot = &mut state.slots[idx];
        // Only discard if the generation matches (slot is still claimed by this packed index).
        if slot.generation == generation {
            slot.generation = slot.generation.wrapping_add(1);
            slot.state = STATE_FREE;
            state.free.push_back(idx);
        }
        // If generation doesn't match, it's already been released or reused, so ignore.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claim_release_single() {
        const SIZE: usize = 128;
        const NUM_SLOTS: usize = 4;
        let pool = SlotPool::<SIZE>::new(NUM_SLOTS);

        // First claim gets slot 0, generation 0.
        let packed = pool.try_claim().unwrap();
        let (idx, generation) = SlotPool::<SIZE>::unpack_index(packed);
        assert_eq!(idx, 0);
        assert_eq!(generation, 0);

        // Write and verify.
        pool.with_payload_mut(packed, |payload| {
            payload[0] = 42;
            assert_eq!(payload[0], 42);
        });

        // Simulate consumption and release.
        {
            let mut state = pool.state.lock();
            let slot = &mut state.slots[idx];
            slot.state = STATE_CONSUMED;
        }
        pool.release_audio(packed);

        // Because the free list is FIFO, the next claim gets slot 1 (not slot 0).
        let packed2 = pool.try_claim().unwrap();
        let (idx2, gen2) = SlotPool::<SIZE>::unpack_index(packed2);
        assert_eq!(idx2, 1);
        assert_eq!(gen2, 0); // first use of slot 1

        // Release that slot too.
        {
            let mut state = pool.state.lock();
            let slot = &mut state.slots[idx2];
            slot.state = STATE_CONSUMED;
        }
        pool.release_audio(packed2);

        // Continue releasing slots 2 and 3 so that slot 0 becomes free again.
        for _ in 0..2 {
            let p = pool.try_claim().unwrap();
            let (i, _) = SlotPool::<SIZE>::unpack_index(p);
            let mut state = pool.state.lock();
            let slot = &mut state.slots[i];
            slot.state = STATE_CONSUMED;
            drop(state); // release lock before calling release
            pool.release_audio(p);
        }

        // Now claim again. The free list has cycled back to slot 0,
        // and its generation must have been incremented.
        let packed3 = pool.try_claim().unwrap();
        let (idx3, gen3) = SlotPool::<SIZE>::unpack_index(packed3);
        assert_eq!(idx3, 0);
        assert_eq!(gen3, 1); // generation incremented after first release
    }

    #[test]
    fn test_concurrent_stress() {
        const SIZE: usize = 128;
        const NUM_SLOTS: usize = 16;
        const ITERATIONS: usize = 10000;
        const NUM_THREADS: usize = 4;

        use std::sync::Arc;
        use std::thread;

        let pool = Arc::new(SlotPool::<SIZE>::new(NUM_SLOTS));
        let mut handles = vec![];

        for t in 0..NUM_THREADS {
            let pool = Arc::clone(&pool);
            handles.push(thread::spawn(move || {
                let mut claimed = 0;
                for _ in 0..ITERATIONS {
                    if let Some(packed) = pool.try_claim() {
                        // Write thread id and verify.
                        pool.with_payload_mut(packed, |payload| {
                            payload[0] = t as u8;
                            assert_eq!(payload[0], t as u8);
                        });
                        // Simulate processing: set state to CONSUMED.
                        {
                            let (idx, _) = SlotPool::<SIZE>::unpack_index(packed);
                            let mut state = pool.state.lock();
                            let slot = &mut state.slots[idx];
                            slot.state = STATE_CONSUMED;
                        }
                        pool.release_audio(packed);
                        claimed += 1;
                    } else {
                        thread::yield_now();
                    }
                }
                claimed
            }));
        }

        let total: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
        assert!(total > 0);
    }
}
