import net from "net";
import * as flatbuffers from "flatbuffers";
// Import the generated TypeScript bindings (the file is schema_generated.ts)
import { Metadata, DomNode, Rect } from "./generated/schema.js";

const ADDR = "127.0.0.1";
const PORT = 8080;

const WIDTH = 1280;
const HEIGHT = 720;
const PIXEL_DATA_SIZE = WIDTH * HEIGHT * 4;
const dummyPixels = Buffer.alloc(PIXEL_DATA_SIZE, 0xaf);

function randStr(l: number): string {
    return Math.random()
        .toString(36)
        .substring(2, 2 + l);
}

interface DomNodeData {
    id: number;
    tag: string;
    has_text: boolean;
    text: string | null;
    rect: { x: number; y: number; width: number; height: number };
}

function generateDomNodes(count: number): DomNodeData[] {
    const nodes: DomNodeData[] = [];
    for (let i = 0; i < count; i++) {
        nodes.push({
            id: i,
            tag: "div",
            has_text: true,
            text: randStr(10),
            rect: {
                x: Math.random() * WIDTH,
                y: Math.random() * HEIGHT,
                width: 50 + Math.random() * 200,
                height: 20 + Math.random() * 100,
            },
        });
    }
    return nodes;
}

const domNodes = generateDomNodes(5000);

function buildFlatBuffer(
    width: number,
    height: number,
    timestamp: bigint,
    nodesArray: DomNodeData[]
): Uint8Array {
    const builder = new flatbuffers.Builder(1024 * 1024);

    const nodeOffsets: number[] = [];
    for (const node of nodesArray) {
        const tagOffset = builder.createString(node.tag);
        const textOffset = node.text ? builder.createString(node.text) : null;
        const rectOffset = Rect.createRect(
            builder,
            node.rect.x,
            node.rect.y,
            node.rect.width,
            node.rect.height
        );
        DomNode.startDomNode(builder);
        DomNode.addId(builder, node.id);
        DomNode.addTag(builder, tagOffset);
        DomNode.addHasText(builder, node.has_text);
        if (textOffset !== null) DomNode.addText(builder, textOffset);
        DomNode.addRect(builder, rectOffset);
        nodeOffsets.push(DomNode.endDomNode(builder));
    }

    const nodesVector = Metadata.createNodesVector(builder, nodeOffsets);
    Metadata.startMetadata(builder);
    Metadata.addTimestamp(builder, timestamp);
    Metadata.addWidth(builder, width);
    Metadata.addHeight(builder, height);
    Metadata.addNodes(builder, nodesVector);
    const metadata = Metadata.endMetadata(builder);
    builder.finish(metadata);

    return builder.asUint8Array();
}

async function runSpike(): Promise<void> {
    const client = new net.Socket();
    console.log(
        `Connecting to Rust Runtime at ${ADDR}:${PORT} [FlatBuffers Mode]...`
    );

    client.connect(PORT, ADDR, () => {
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

function sendFrame(socket: net.Socket): void {
    const frameStart = performance.now();

    const fbBytes = buildFlatBuffer(
        WIDTH,
        HEIGHT,
        BigInt(Date.now()),
        domNodes
    );
    const fbLenBuf = Buffer.alloc(4);
    fbLenBuf.writeUInt32LE(fbBytes.length);

    socket.write(fbLenBuf);
    socket.write(Buffer.from(fbBytes));
    socket.write(dummyPixels);

    socket.once("data", (data: Buffer) => {
        if (data[0] === 0x01) {
            const elapsed = performance.now() - frameStart;
            frameCount++;
            if (frameCount % 60 === 0) {
                console.log(
                    `Frame ${frameCount} | Total Latency: ${elapsed.toFixed(2)}ms | FB Size: ${(fbBytes.length / 1024).toFixed(2)}KB`
                );
            }
            setImmediate(() => sendFrame(socket));
        }
    });
}

runSpike();
