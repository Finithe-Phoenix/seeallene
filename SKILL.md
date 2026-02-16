---
name: seealln
description: Cross-platform (Windows/macOS) local "eyes + hands" automation runner for desktop apps and VDI windows using screenshot streaming (MJPEG), OCR (EasyOCR), and safe input (mouse/keyboard) with guardrails. Use when you need to (1) stream the current screen/region via localhost, (2) infer clicks/keystrokes from what’s on screen (OCR + heuristics), (3) navigate UIs robustly even when focus/keys fail, and (4) batch email triage (e.g., Outlook) with confirmation gates.
---

# SeeAlln (Eyes + Hands)

Implementa un runner **local** (localhost) para “ojos y manos” con guardrails.

## Reglas (obligatorias)

- **No exponer a internet**: bind siempre en `127.0.0.1` por defecto.
- **Kill switch**: `Ctrl+Alt+Q` (Windows) / `Ctrl+Option+Q` (macOS) (configurable).
- **Acciones sensibles** (borrar/enviar/submit/aprobar/descargar/ejecutar) requieren confirmación humana.
- **Region lock**: preferir región/ventana (no pantalla completa) cuando sea posible.
- **Login/MFA/CAPTCHA**: detener y pedir control humano.

## Quick start (local)

1) Inicia el stream local (MJPEG):

- `python scripts/stream_server.py`  
  Endpoints:
  - `http://127.0.0.1:8765/`
  - `http://127.0.0.1:8765/snapshot.jpg`
  - `http://127.0.0.1:8765/stream?fps=10&q=60`

2) Inicia el watchdog (reinicia el stream si cae):

- `python scripts/watchdog_stream_server.py`

3) Inicia el controlador API (intents):

- `python scripts/controller_server.py`

## Intents soportados (MVP)

- `GET /health`
- `GET /snapshot` (proxy a snapshot.jpg)
- `POST /intent/next_email` (navegación robusta: ↓ y fallback por clic)
- `POST /intent/capture_batch` (captura N correos con verificación de cambio)

## macOS notes

- Requiere permisos de **Accessibility** (input) y **Screen Recording** (captura) para funcionar.

## Desarrollo

- Scripts viven en `scripts/`.
- Threat model y defaults: ver `references/`.
