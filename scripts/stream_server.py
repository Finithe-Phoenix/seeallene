import asyncio
import os
from aiohttp import web
import mss
from PIL import Image
import io
import json
from datetime import datetime

BASE_DIR = os.path.dirname(__file__)
CONFIG_PATH = os.path.join(BASE_DIR, "config.json")


def load_bbox():
    try:
        with open(CONFIG_PATH, "r", encoding="utf-8") as f:
            cfg = json.load(f)
        return cfg.get("bbox")
    except Exception:
        return None


def grab_jpeg(bbox=None, quality=70):
    with mss.mss() as sct:
        if bbox:
            mon = {"left": bbox["left"], "top": bbox["top"], "width": bbox["width"], "height": bbox["height"]}
        else:
            # Primary monitor full screen
            mon = sct.monitors[1]
        img = sct.grab(mon)
        im = Image.frombytes("RGB", img.size, img.rgb)
        buf = io.BytesIO()
        im.save(buf, format="JPEG", quality=quality, optimize=True)
        return buf.getvalue()


async def handle_root(request):
    return web.json_response({
        "ok": True,
        "ts": datetime.now().isoformat(),
        "endpoints": {
            "stream": "/stream",
            "snapshot": "/snapshot.jpg"
        },
        "bind": "127.0.0.1",
        "note": "Local-only MJPEG stream. Do not expose publicly."
    })


async def handle_snapshot(request):
    bbox = load_bbox()
    jpg = grab_jpeg(bbox=bbox, quality=75)
    return web.Response(body=jpg, content_type="image/jpeg")


async def handle_stream(request):
    bbox = load_bbox()
    boundary = "frame"
    resp = web.StreamResponse(status=200, reason='OK', headers={
        'Content-Type': f'multipart/x-mixed-replace; boundary={boundary}',
        'Cache-Control': 'no-cache',
        'Connection': 'close',
        'Pragma': 'no-cache',
    })
    await resp.prepare(request)

    fps = float(request.query.get("fps", "10"))
    fps = max(1.0, min(15.0, fps))
    delay = 1.0 / fps

    quality = int(request.query.get("q", "60"))
    quality = max(30, min(85, quality))

    try:
        while True:
            jpg = grab_jpeg(bbox=bbox, quality=quality)
            await resp.write(f"--{boundary}\r\n".encode())
            await resp.write(b"Content-Type: image/jpeg\r\n")
            await resp.write(f"Content-Length: {len(jpg)}\r\n\r\n".encode())
            await resp.write(jpg)
            await resp.write(b"\r\n")
            await asyncio.sleep(delay)
    except (asyncio.CancelledError, ConnectionResetError, BrokenPipeError):
        pass
    except Exception:
        pass

    return resp


def main():
    app = web.Application()
    app.router.add_get('/', handle_root)
    app.router.add_get('/stream', handle_stream)
    app.router.add_get('/snapshot.jpg', handle_snapshot)

    web.run_app(app, host='127.0.0.1', port=8765, access_log=None)


if __name__ == '__main__':
    main()
