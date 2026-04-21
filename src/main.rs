use std::error::Error;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

mod protocol;
use protocol::{ContentBuffer, Metadata};

const ADDR: &str = "127.0.0.1:8080";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind(ADDR).await?;
    println!("Listening on {} [Optimized Header/Payload Mode]...", ADDR);

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let mut s = stream;
            if let Err(e) = handle_connection(&mut s).await {
                eprintln!("Connection error: {}", e);
            }
        });
    }
}

async fn handle_connection(stream: &mut TcpStream) -> Result<(), Box<dyn Error>> {
    let mut len_buf = [0u8; 4];

    // Pre-allocate buffers to avoid runtime allocation jitter
    let mut meta_payload = Vec::with_capacity(1024); // Metadata is tiny
    let mut pixel_payload = Vec::with_capacity(1920 * 1080 * 4); // 1080p baseline

    loop {
        // 1. Read Metadata Length (u32 LE)
        if stream.read_exact(&mut len_buf).await.is_err() {
            break;
        }
        let meta_len = u32::from_le_bytes(len_buf) as usize;

        // 2. Read and Deserialize Metadata
        unsafe {
            meta_payload.set_len(meta_len);
        }
        stream.read_exact(&mut meta_payload).await?;
        let meta: Metadata = rmp_serde::from_slice(&meta_payload)?;

        // 3. Calculate and Read Raw Pixels
        let pixel_bytes = (meta.width * meta.height * 4) as usize;
        unsafe {
            pixel_payload.set_len(pixel_bytes);
        }

        let start_time = Instant::now();
        stream.read_exact(&mut pixel_payload).await?;
        let io_time = start_time.elapsed();

        // 4. Construct ContentBuffer (Zero-copy reference to pixel_payload)
        let _container = ContentBuffer {
            meta,
            pixel_data: &pixel_payload,
        };

        println!(
            "Frame: {}x{} | Meta: {}B | Pixel IO: {:?}",
            _container.meta.width, _container.meta.height, meta_len, io_time
        );

        // 5. Backpressure ACK
        stream.write_u8(0x01).await?;
    }
    Ok(())
}
