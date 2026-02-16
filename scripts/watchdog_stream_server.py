import time
import subprocess
import sys
import urllib.request

URL = "http://127.0.0.1:8765/"
CMD = [sys.executable, "stream_server.py"]


def is_up(timeout=1.5):
    try:
        with urllib.request.urlopen(URL, timeout=timeout) as r:
            return r.status == 200
    except Exception:
        return False


def main():
    p = None
    while True:
        if not is_up():
            if p and p.poll() is None:
                try:
                    p.terminate()
                except Exception:
                    pass
            try:
                p = subprocess.Popen(CMD)
            except Exception:
                p = None
        time.sleep(5)


if __name__ == "__main__":
    main()
