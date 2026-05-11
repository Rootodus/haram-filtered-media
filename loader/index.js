import net from "net";
import { encode } from "@msgpack/msgpack";

const ADDR = "127.0.0.1";
const PORT = 8080;

// 1. Dimensions: Set to match the Rust spike target
const WIDTH = 1280;
const HEIGHT = 720;
const PIXEL_DATA_SIZE = WIDTH * HEIGHT * 4;

const dummyPixels = Buffer.alloc(PIXEL_DATA_SIZE, 0xaf);

const randStr = (l) =>
    Math.random()
        .toString(36)
        .substring(2, 2 + l);

const generateDomNodes = (count) => {
    return Array.from({ length: count }, (_, i) => ({
        id: i,
        tag: "div",
        text: randStr(10),
        rect: [Math.random(), Math.random(), 100, 100],
    }));
};

const domNodes = generateDomNodes(5000);

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

    // 1. Metadata
    const metaPayload = encode({
        timestamp: Date.now(),
        width: WIDTH,
        height: HEIGHT,
        node_count: domNodes.length,
    });
    const metaLenBuf = Buffer.alloc(4);
    metaLenBuf.writeUInt32LE(metaPayload.length);

    // 2. DOM Nodes (The Stress Payload)
    const domPayload = encode(domNodes);
    const domLenBuf = Buffer.alloc(4);
    domLenBuf.writeUInt32LE(domPayload.length);

    // 3. Sequential Write [MetaLen][Meta][DomLen][Dom][Pixels]
    socket.write(metaLenBuf);
    socket.write(metaPayload);
    socket.write(domLenBuf);
    socket.write(domPayload);
    socket.write(dummyPixels);

    socket.once("data", (data) => {
        if (data[0] === 0x01) {
            const elapsed = performance.now() - frameStart;
            frameCount++;
            if (frameCount % 60 === 0) {
                console.log(
                    `Frame ${frameCount} | Total Latency: ${elapsed.toFixed(2)}ms | DOM Size: ${(domPayload.length / 1024).toFixed(2)}KB`
                );
            }
            setImmediate(() => sendFrame(socket));
        }
    });
}

runSpike();
