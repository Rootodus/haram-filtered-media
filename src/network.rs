use crate::protocol::SharedAppState;
use crate::schema::Metadata;

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

static INFERENCE_RUNNING: AtomicBool = AtomicBool::new(false);
static SKIP_NEXT_INFERENCE: AtomicBool = AtomicBool::new(false);

pub async fn handle_connection(
    mut stream: TcpStream,
    state: Arc<SharedAppState>,
    mut ack_receiver: tokio::sync::mpsc::Receiver<()>,
) -> Result<(), Box<dyn Error>> {
    let _ = stream.set_nodelay(true);
    let mut len_buf = [0u8; 4];

    loop {
        // Read FlatBuffer length prefix
        if stream.read_exact(&mut len_buf).await.is_err() {
            break;
        }
        let fb_len = u32::from_le_bytes(len_buf) as usize;
        let mut fb_bytes = vec![0u8; fb_len];
        if stream.read_exact(&mut fb_bytes).await.is_err() {
            break;
        }

        // --- FlatBuffers verification timing ---
        let verify_start = Instant::now();
        // SAFETY: The loader is trusted. FlatBuffer is built by the same system.
        let metadata = unsafe { flatbuffers::root_unchecked::<Metadata>(&fb_bytes) };
        let verify_dur = verify_start.elapsed();

        let timestamp = metadata.timestamp();
        let width = metadata.width();
        let height = metadata.height();

        // --- Node vector length access (O(1), no iteration) ---
        let node_len_start = Instant::now();
        let nodes_opt = metadata.nodes();
        let node_count = nodes_opt.map(|v| v.len()).unwrap_or(0);
        let node_len_dur = node_len_start.elapsed();

        // Read raw pixel data
        let pixel_bytes = (width * height * 4) as usize;
        let mut pixel_vec = vec![0u8; pixel_bytes];
        if stream.read_exact(&mut pixel_vec).await.is_err() {
            break;
        }

        // Convert to Arc<[u8]>
        let fb_arc = Arc::from(fb_bytes.into_boxed_slice());
        let pixel_arc = Arc::from(pixel_vec.into_boxed_slice());

        state.update_frame(timestamp, width, height, fb_arc, pixel_arc);

        if INFERENCE_RUNNING.load(Ordering::Relaxed) {
            SKIP_NEXT_INFERENCE.store(true, Ordering::Release);
        }

        static LOG_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let count = LOG_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if count == 0 || count % 100 == 0 {
            println!(
                "Rust: verify={:?}, node_count={}, node_len_access={:?}, fb_bytes={}",
                verify_dur, node_count, node_len_dur, fb_len
            );
        }

        // Wait for GPU ACK and reply
        if ack_receiver.recv().await.is_none() {
            break;
        }
        if stream.write_all(&[0x01]).await.is_err() {
            break;
        }
    }
    Ok(())
}
