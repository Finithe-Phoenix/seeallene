# SeeAlln — Eyes + Hands (Local-only)

Local (localhost-only) desktop automation runner: **screen streaming + OCR + safe input**.

## Security defaults
- Binds to `127.0.0.1` only.
- Kill switch required.
- Sensitive actions (delete/send/approve/run/download) must be gated.
- Do **not** bypass MFA/CAPTCHA.

## Quick start (Rust via Docker — recommended)

This is the most "integrable" path: one container, localhost-only.

```bash
docker compose up --build
```

Endpoints:
- Health: `http://127.0.0.1:8765/health`
- Snapshot: `http://127.0.0.1:8765/snapshot.jpg`
- Stream (MJPEG): `http://127.0.0.1:8765/stream?fps=10&q=60`

## Quick start (Python runner)

```bash
python -m pip install -r requirements.txt
python scripts/run_all.py
```

Python endpoints:
- Stream: `http://127.0.0.1:8765/stream?fps=10&q=60`
- Snapshot: `http://127.0.0.1:8765/snapshot.jpg`
- Controller: `http://127.0.0.1:8766/health`

## macOS permissions (required for hands)
1) System Settings → Privacy & Security
2) Enable for your terminal/Python:
   - **Screen Recording** (for capture)
   - **Accessibility** (for mouse/keyboard)

## Notes
- Windows: run as the logged-in desktop user (not a service account) for UI automation.
- Do not expose ports publicly. This project binds to `127.0.0.1` by default.
