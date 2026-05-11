import net from "net";
import { encode } from "@msgpack/msgpack";

const ADDR = "127.0.0.1";
const PORT = 8080;

// 1. Dimensions: Set to match the Rust spike target
const WIDTH = 1280;
const HEIGHT = 720;
const PIXEL_DATA_SIZE = WIDTH * HEIGHT * 4;

const dummyPixels = Buffer.alloc(PIXEL_DATA_SIZE, 0xaf);

async function runSpike() {
    const client = new net.Socket();

    console.log(
        `Connecting to Rust Runtime at ${ADDR}:${PORT} [Optimized Stream Mode]...`
    );

    client.connect(PORT, ADDR, () => {
        // 2. Jitter Reduction: Immediately disable Nagle's Algorithm
        client.setNoDelay(true);
        console.log("Connected. Starting throughput test...");
        sendFrame(client);
    });

    client.on("error", (err) => {
        console.error("Socket Error:", err.message);
        process.exit(1);
    });
}

let frameCount = 0;
let startTime = Date.now();

function sendFrame(socket) {
    const frameStart = performance.now();

    // 3. Protocol Enforcement: Prepare Metadata
    const metadata = {
        timestamp: Date.now(),
        width: WIDTH,
        height: HEIGHT,
    };

    // Encode MessagePack
    const serializedMeta = encode(metadata);

    // Efficiency: Wrap the Uint8Array in a Buffer without copying
    const metaPayload = Buffer.from(
        serializedMeta.buffer,
        serializedMeta.byteOffset,
        serializedMeta.byteLength
    );

    // Create 4-byte Little-Endian u32 for metadata length
    const metaLenBuf = Buffer.alloc(4);
    metaLenBuf.writeUInt32LE(metaPayload.length);

    // 4. Sequential Write: Protocol sequence [Len] -> [Meta] -> [Pixels]
    // Node.js will buffer these and send them as a stream
    socket.write(metaLenBuf);
    socket.write(metaPayload);
    socket.write(dummyPixels);

    // 5. Flow Control: Wait for 1-byte ACK (0x01) from Rust
    socket.once("data", (data) => {
        if (data[0] === 0x01) {
            const elapsed = performance.now() - frameStart;
            frameCount++;

            // Throttled logging to keep console overhead low
            if (frameCount % 60 === 0) {
                const totalElapsed = (Date.now() - startTime) / 1000;
                const fps = (frameCount / totalElapsed).toFixed(2);
                console.log(
                    `Frame ${frameCount} | Latency: ${elapsed.toFixed(2)}ms | Avg FPS: ${fps}`
                );
            }

            // Trigger next frame immediately after ACK
            setImmediate(() => sendFrame(socket));
        }
    });
}

runSpike();
