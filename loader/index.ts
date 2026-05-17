import net from "net";
import * as flatbuffers from "flatbuffers";
import puppeteer from "puppeteer";
import { Metadata, DomNode, Rect } from "./generated/schema.js";

const ADDR = "127.0.0.1";
const PORT = 8080;
const TARGET_URL = "https://en.wikipedia.org/wiki/HTML5";
const TARGET_SELECTOR = "p";

const WIDTH = 1280;
const HEIGHT = 720;
const PIXEL_DATA_SIZE = WIDTH * HEIGHT * 4;
const dummyPixels = Buffer.alloc(PIXEL_DATA_SIZE, 0xaf);

interface DomNodeData {
    id: number;
    tag: string;
    has_text: boolean;
    text: string | null;
    rect: { x: number; y: number; width: number; height: number };
}

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

async function extractDomNodes(
    page: puppeteer.Page,
    selector: string
): Promise<{ nodes: DomNodeData[]; count: number; samples: string[] }> {
    const result = await page.evaluate((sel) => {
        const elements = document.querySelectorAll(sel);
        const nodes: Array<{
            id: number;
            tag: string;
            has_text: boolean;
            text: string | null;
            rect: { x: number; y: number; width: number; height: number };
        }> = [];
        let id = 0;
        for (const el of elements) {
            const rect = el.getBoundingClientRect();
            const text = el.textContent?.trim() ?? null;
            nodes.push({
                id: id++,
                tag: el.tagName.toLowerCase(),
                has_text: text !== null && text.length > 0,
                text: text ? text.slice(0, 500) : null,
                rect: {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                },
            });
        }
        // Collect sample information (first 3 elements)
        const samples = Array.from(elements)
            .slice(0, 3)
            .map(
                (el) =>
                    `${el.tagName} (${el.getBoundingClientRect().x},${el.getBoundingClientRect().y})`
            );
        return { nodes, count: nodes.length, samples };
    }, selector);
    console.log(`Raw elements found: ${result.count}`);
    for (const sample of result.samples) {
        console.log(`Sample: ${sample}`);
    }
    return result;
}

async function runSpike(): Promise<void> {
    const browser = await puppeteer.launch({ headless: true });
    const page = await browser.newPage();
    await page.setViewport({ width: 1920, height: 1080 });
    await page.goto(TARGET_URL, { waitUntil: "networkidle2" });
    await page.waitForSelector(TARGET_SELECTOR, { timeout: 5000 });

    const { nodes: domNodes } = await extractDomNodes(page, TARGET_SELECTOR);
    console.log(
        `Extracted ${domNodes.length} nodes matching selector "${TARGET_SELECTOR}"`
    );

    await browser.close();

    const client = new net.Socket();
    console.log(
        `Connecting to Rust Runtime at ${ADDR}:${PORT} [Real DOM Extraction]...`
    );
    client.connect(PORT, ADDR, () => {
        client.setNoDelay(true);
        console.log("Connected. Sending snapshot...");
        sendSnapshot(client, domNodes);
    });
    client.on("error", (err) => {
        console.error("Socket Error:", err.message);
        process.exit(1);
    });
}

let frameCount = 0;

function sendSnapshot(socket: net.Socket, domNodes: DomNodeData[]): void {
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
            frameCount++;
            console.log(
                `Snapshot ${frameCount} | ACK received | FB Size: ${(fbBytes.length / 1024).toFixed(2)}KB`
            );
            socket.end();
        }
    });
}

runSpike();
