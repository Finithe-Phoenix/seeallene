# SeeAlln — Eyes + Hands (Local-only)

Local (localhost-only) desktop automation runner: **screen streaming + OCR + safe input**.

## Security defaults
- Binds to `127.0.0.1` only.
- Kill switch required.
- Sensitive actions (delete/send/approve/run/download) must be gated.
- Do **not** bypass MFA/CAPTCHA.

## Quick start
```bash
python -m pip install -r requirements.txt
python scripts/stream_server.py
python scripts/watchdog_stream_server.py
python scripts/controller_server.py
```

Endpoints:
- Stream: `http://127.0.0.1:8765/stream?fps=10&q=60`
- Snapshot: `http://127.0.0.1:8765/snapshot.jpg`
- Controller: `http://127.0.0.1:8766/health`

## Notes
- macOS requires Accessibility + Screen Recording permissions.
- Windows may require running as the logged-in desktop user.
