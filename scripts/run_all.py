import subprocess
import sys
import time
import os

HERE = os.path.dirname(__file__)


def start(name, args):
    p = subprocess.Popen([sys.executable, os.path.join(HERE, *args)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    print(f"[{name}] pid={p.pid}")
    return p


def main():
    print("Starting SeeAlln services (localhost-only)...")
    p_stream = start("stream", ["stream_server.py"])
    time.sleep(0.5)
    p_watchdog = start("watchdog", ["watchdog_stream_server.py"])
    time.sleep(0.5)
    p_ctrl = start("controller", ["controller_server.py"])

    print("\nEndpoints:")
    print("- Stream:    http://127.0.0.1:8765/stream?fps=10&q=60")
    print("- Snapshot:  http://127.0.0.1:8765/snapshot.jpg")
    print("- Controller:http://127.0.0.1:8766/health")
    print("\nCtrl+C to stop.")

    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        pass

    for p in (p_ctrl, p_watchdog, p_stream):
        try:
            p.terminate()
        except Exception:
            pass


if __name__ == "__main__":
    main()
