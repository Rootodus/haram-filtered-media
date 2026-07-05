import net from "net";
import * as flatbuffers from "flatbuffers";
import puppeteer from "puppeteer";
import { PNG } from "pngjs";
import { Metadata, DomNode, Rect } from "./generated/schema.js";

const ADDR = "127.0.0.1";
const PORT = 8080;
const TARGET_URL = "https://en.wikipedia.org/wiki/HTML5";
const TARGET_SELECTOR = "p";
const VIEWPORT_WIDTH = 1280;
const VIEWPORT_HEIGHT = 720;

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
): Promise<DomNodeData[]> {
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
        return nodes;
    }, selector);
    console.log(
        `Extracted ${result.length} nodes matching selector "${selector}"`
    );
    return result;
}

async function runSpike(): Promise<void> {
    // Launch browser and capture screenshot
    const browser = await puppeteer.launch({ headless: true });
    const page = await browser.newPage();
    await page.setViewport({ width: VIEWPORT_WIDTH, height: VIEWPORT_HEIGHT });
    await page.goto(TARGET_URL, { waitUntil: "networkidle2" });
    await page.waitForSelector(TARGET_SELECTOR, { timeout: 5000 });

    // Extract DOM nodes
    const domNodes = await extractDomNodes(page, TARGET_SELECTOR);

    // Capture screenshot as PNG
    const screenshotBuffer = await page.screenshot({
        encoding: "binary",
        type: "png",
    });
    const png = PNG.sync.read(screenshotBuffer);
    const pixelBuffer = png.data; // Uint8Array of RGBA
    const actualWidth = png.width;
    const actualHeight = png.height;

    console.log(
        `Screenshot: ${actualWidth}x${actualHeight}, pixel buffer size: ${pixelBuffer.length}`
    );

    await browser.close();

    // Connect to Rust runtime
    const client = new net.Socket();
    console.log(
        `Connecting to Rust Runtime at ${ADDR}:${PORT} [Real DOM Extraction]...`
    );
    client.connect(PORT, ADDR, () => {
        client.setNoDelay(true);
        console.log("Connected. Sending snapshot...");
        sendSnapshot(client, domNodes, actualWidth, actualHeight, pixelBuffer);
    });
    client.on("error", (err) => {
        console.error("Socket Error:", err.message);
        process.exit(1);
    });
}

let frameCount = 0;

function sendSnapshot(
    socket: net.Socket,
    domNodes: DomNodeData[],
    width: number,
    height: number,
    pixelBuffer: Uint8Array
): void {
    const fbBytes = buildFlatBuffer(
        width,
        height,
        BigInt(Date.now()),
        domNodes
    );
    const fbLenBuf = Buffer.alloc(4);
    fbLenBuf.writeUInt32LE(fbBytes.length);

    // Write: [FB_Length][FlatBuffer_Payload][Raw_Pixels]
    socket.write(fbLenBuf);
    socket.write(Buffer.from(fbBytes));
    socket.write(Buffer.from(pixelBuffer));

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
