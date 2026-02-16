import time
import urllib.request
import numpy as np
import cv2
import pyautogui
import easyocr

URL = "http://127.0.0.1:8765/snapshot.jpg"

# Initialize OCR reader once (Spanish + English helps with mixed UI)
reader = easyocr.Reader(['es', 'en'], gpu=False)


def fetch_snapshot():
    with urllib.request.urlopen(URL, timeout=5) as r:
        data = r.read()
    img = cv2.imdecode(np.frombuffer(data, np.uint8), cv2.IMREAD_COLOR)
    return img


def ocr_text(img):
    # Downscale for speed
    h, w = img.shape[:2]
    scale = 0.6
    small = cv2.resize(img, (int(w * scale), int(h * scale)))
    results = reader.readtext(small)
    texts = [t[1] for t in results]
    return "\n".join(texts).lower(), results, scale


def is_outlook(img):
    txt, _, _ = ocr_text(img)
    # Spanish UI markers
    return ("archivo" in txt and "inicio" in txt) or ("bandeja" in txt and "entrada" in txt)


def get_subject_line(img):
    # crude: take OCR from upper-right reading pane area
    h, w = img.shape[:2]
    roi = img[int(0.12 * h): int(0.25 * h), int(0.48 * w): int(0.98 * w)]
    txt, _, _ = ocr_text(roi)
    first = txt.splitlines()[0].strip() if txt.strip() else ""
    return first


def click_next_in_list(img):
    """Fallback: click slightly below current selected item in message list area."""
    h, w = img.shape[:2]
    x = int(0.33 * w)
    y = int(0.35 * h)
    pyautogui.click(x, y)
    time.sleep(0.2)
    pyautogui.click(x, y + int(0.08 * h))


def robust_next(max_tries=2):
    img0 = fetch_snapshot()
    if not is_outlook(img0):
        raise SystemExit("No veo Outlook al frente. Pon Outlook al frente y vuelve a intentar.")

    subj0 = get_subject_line(img0)

    for _ in range(max_tries):
        pyautogui.press('down')
        time.sleep(1.2)
        img1 = fetch_snapshot()
        subj1 = get_subject_line(img1)
        if subj1 and subj1 != subj0:
            return True

    click_next_in_list(img0)
    time.sleep(1.5)
    img2 = fetch_snapshot()
    subj2 = get_subject_line(img2)
    return bool(subj2 and subj2 != subj0)


if __name__ == '__main__':
    ok = robust_next()
    print("OK" if ok else "NO_CHANGE")
