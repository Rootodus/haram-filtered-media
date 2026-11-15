# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "websockets",
# ]
# ///

import asyncio
import json
import websockets

async def test_server():
    uri = "ws://127.0.0.1:8377/ws"
    print(f"Connecting to {uri}...")
    
    try:
        async with websockets.connect(uri) as websocket:
            # Test 1: Clean text (should pass through mostly unchanged)
            clean_text = "This is a perfectly normal and polite sentence."
            print(f"\nSending: {clean_text}")
            await websocket.send(json.dumps({"text": clean_text}))
            response = await websocket.recv()
            print(f"Response: {response}")

            # Test 2: Text with profanity (should be rewritten or redacted)
            toxic_text = "Shut the fuck up and fix this stupid code, it's completely broken."
            print(f"\nSending: {toxic_text}")
            await websocket.send(json.dumps({"text": toxic_text}))
            response = await websocket.recv()
            print(f"Response: {response}")

    except Exception as e:
        print(f"Failed to connect: {e}")
        print("Make sure the server is running and the GUI says 'Server is running'.")

if __name__ == "__main__":
    asyncio.run(test_server())
