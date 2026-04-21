import net from "net";
import { encode } from "@msgpack/msgpack";

const ADDR = "127.0.0.1";
const PORT = 8080;

const WIDTH = 1920;
const HEIGHT = 1080;
const PIXEL_DATA_SIZE = WIDTH * HEIGHT * 4;

async function runSpike() {
    const client = new net.Socket();

    console.log(
        `Connecting to Rust Runtime at ${ADDR}:${PORT} [Optimized Stream Mode]...`
    );

    client.connect(PORT, ADDR, () => {
        console.log("Connected. Starting throughput test...");
        sendFrame(client);
    });

    client.on("error", (err) => {
        console.error("Socket Error:", err.message);
        process.exit(1);
    });
}

const dummyPixels = Buffer.alloc(PIXEL_DATA_SIZE, 0xaf);

let frameCount = 0;
let startTime = Date.now();

function sendFrame(socket) {
    const frameStart = performance.now();

    // 1. Prepare Metadata (Small object, fast encoding)
    const metadata = {
        timestamp: Date.now(),
        width: WIDTH,
        height: HEIGHT,
    };

    // 2. Serialize Metadata only
    const serializedMeta = encode(metadata);
    const metaPayload = Buffer.from(
        serializedMeta.buffer,
        serializedMeta.byteOffset,
        serializedMeta.byteLength
    );

    // 3. Create Metadata Length Prefix (u32 LE)
    const metaLenBuf = Buffer.alloc(4);
    metaLenBuf.writeUInt32LE(metaPayload.length);

    // 4. Sequential Write: [MetaLen] -> [MetaPayload] -> [RawPixels]
    // The original dummyPixels buffer is written directly without encoding
    socket.write(metaLenBuf);
    socket.write(metaPayload);
    socket.write(dummyPixels);

    // 5. Wait for 1-byte ACK (0x01)
    socket.once("data", (data) => {
        if (data[0] === 0x01) {
            const elapsed = performance.now() - frameStart;
            frameCount++;

            if (frameCount % 10 === 0) {
                const totalElapsed = (Date.now() - startTime) / 1000;
                const fps = (frameCount / totalElapsed).toFixed(2);
                console.log(
                    `Frame ${frameCount} | Latency: ${elapsed.toFixed(2)}ms | Avg FPS: ${fps}`
                );
            }
            setImmediate(() => sendFrame(socket));
        }
    });
}

runSpike();
