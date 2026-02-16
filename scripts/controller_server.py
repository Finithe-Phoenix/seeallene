import json
import urllib.request
from aiohttp import web

from robust_nav import robust_next

SNAP_URL = "http://127.0.0.1:8765/snapshot.jpg"


def fetch_snapshot_bytes():
    with urllib.request.urlopen(SNAP_URL, timeout=5) as r:
        return r.read()


async def health(request):
    return web.json_response({"ok": True})


async def snapshot(request):
    data = fetch_snapshot_bytes()
    return web.Response(body=data, content_type="image/jpeg")


async def intent_next_email(request):
    # Executes local UI navigation (requires host permissions)
    ok = robust_next()
    return web.json_response({"ok": bool(ok)})


async def main():
    app = web.Application()
    app.router.add_get('/health', health)
    app.router.add_get('/snapshot', snapshot)
    app.router.add_post('/intent/next_email', intent_next_email)

    web.run_app(app, host='127.0.0.1', port=8766, access_log=None)


if __name__ == '__main__':
    import asyncio
    asyncio.run(main())
