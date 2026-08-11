use mlfb_av_core::memory::{STATE_CONSUMED, STATE_INGESTED, SlotPool};
use std::sync::Arc;
use std::thread;

const AUDIO_SLOT_SIZE: usize = 4096;
const NUM_SLOTS: usize = 64;
const ITERATIONS: usize = 100_000;
const NUM_THREADS: usize = 8;

fn main() {
    let pool = Arc::new(SlotPool::<AUDIO_SLOT_SIZE>::new(NUM_SLOTS));
    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let pool = Arc::clone(&pool);
        handles.push(thread::spawn(move || {
            let mut local_claims = 0;
            for _ in 0..ITERATIONS {
                if let Some(packed) = pool.try_claim() {
                    // Write thread id and verify.
                    pool.with_payload_mut(packed, |payload| {
                        payload[0] = t as u8;
                        assert_eq!(payload[0], t as u8);
                    });
                    // Transition to CONSUMED (simulate ML processing).
                    pool.transition_state(packed, STATE_INGESTED, STATE_CONSUMED)
                        .expect("transition failed");
                    // Release.
                    pool.release_audio(packed);
                    local_claims += 1;
                } else {
                    thread::yield_now();
                }
            }
            local_claims
        }));
    }

    let total: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
    println!("Total claims: {}", total);
    assert!(total > 0);
    println!("PASSED");
}
