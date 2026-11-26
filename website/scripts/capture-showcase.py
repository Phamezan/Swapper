"""Regenerate the landing page's product images from the live demo pages.

Usage (from website/, with the site served, for example `npm run dev -- --port 4411`):
    python scripts/capture-showcase.py http://127.0.0.1:4411 [screen ...]

Name screens (for example `remote mods`) to redo only those and their crops; the tray is then skipped.

Needs Python Playwright (with Chromium) and Pillow with AVIF support.
- Every screen renders the Fullsized App at 1440x900 (its real desktop layout) at DPR 2
  with the clock frozen so dates and "since" times never drift.
- Each screen must show its expected text, and none of its forbidden text, or the script exits 1.
- Crops come from scripts/showcase.json (CSS px boxes) and are saved at 2x. Output: public/showcase/.
"""
import io
import json
import sys
import time
from datetime import datetime
from pathlib import Path

from PIL import Image
from playwright.sync_api import Page, TimeoutError as PwTimeout, sync_playwright

ROOT = Path(__file__).resolve().parent
SPEC = json.loads((ROOT / "showcase.json").read_text(encoding="utf-8"))
OUT = ROOT.parent / "public" / "showcase"
BASE = (sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:4411").rstrip("/")
CLOCK = datetime(2026, 11, 24, 21, 30)
APP = SPEC["app"]
ONLY = set(sys.argv[2:])


def open_app(page: Page, query: str) -> None:
    page.goto(f"{BASE}/demo/full?{query}", wait_until="networkidle")


def nav(page: Page, label: str) -> None:
    page.get_by_role("button", name=label, exact=True).first.click()


def to_match(page: Page) -> None:
    open_app(page, "active=kestrel")
    page.get_by_role("button", name="Open Aatrox match details").first.click()
    page.get_by_text("Scoreboard", exact=True).first.click()


def to_matchups(page: Page) -> None:
    open_app(page, "active=vesper")  # Ahri vs Zed mid has the shipped written guide
    nav(page, "Matchups")
    page.get_by_text("Ahri", exact=True).first.click()
    page.get_by_role("button", name="Zed, choose as opponent").first.click()
    page.get_by_text("Guide", exact=True).first.click()


# name: (how to get there, text that must show, text that must not)
SCREENS = {
    "dashboard": (lambda p: open_app(p, "active=kestrel"), ["Champions & roles", "Recent games"], []),
    "draft": (lambda p: open_app(p, "draft-guide"), ["Most played", "Your matchup"], []),
    "match": (to_match, ["Victory", "Defeat"], []),
    "live": (lambda p: open_app(p, "live"), ["Live game", "Red team"], []),
    "matchups": (to_matchups, ["Three things to remember", "Who has the edge?"], ["No written guide yet", "Fixture content"]),
}


def check(page: Page, name: str, expect: list[str], forbid: list[str]) -> None:
    for text in expect:
        try:
            page.get_by_text(text, exact=False).first.wait_for(state="visible", timeout=20000)
        except PwTimeout:
            sys.exit(f"FAIL {name}: expected text not found: {text!r}")
    for text in forbid:
        if page.get_by_text(text, exact=False).count():
            sys.exit(f"FAIL {name}: forbidden text shown: {text!r}")


def save(img: Image.Image, stem: str) -> None:
    for ext, opts in (("avif", {"quality": 55}), ("webp", {"quality": 80, "method": 6})):
        path = OUT / f"{stem}.{ext}"
        img.save(path, ext.upper(), **opts)
        print(f"  {path.name}: {img.width}x{img.height}, {path.stat().st_size // 1024} KB")


def capture(browser, name: str) -> Image.Image:
    run, expect, forbid = SCREENS[name]
    dpr = SPEC["screens"][name]["dpr"]
    page = browser.new_page(viewport=APP, device_scale_factor=dpr)
    page.clock.set_fixed_time(CLOCK)
    run(page)
    check(page, name, expect, forbid)
    page.mouse.move(0, APP["height"] - 1)  # no hover or pressed state in the shot
    time.sleep(2.5)  # art and icons settle after the content check passes
    img = Image.open(io.BytesIO(page.screenshot())).convert("RGB")
    page.close()
    print(f"{name}: {img.width}x{img.height} at DPR {dpr}, content check passed")
    return img


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as p:
        browser = p.chromium.launch()
        shots = {name: capture(browser, name) for name in SCREENS if not ONLY or name in ONLY}
        if ONLY:
            browser.close()
            tray_img = None
        else:
            tray_img = capture_tray(browser)

    for name, img in shots.items():
        for w in SPEC["screens"][name]["widths"]:
            save(img.resize((w, round(img.height * w / img.width)), Image.LANCZOS), f"{name}-{w}")
    for crop_id, crop in SPEC["crops"].items():
        if crop["screen"] not in shots:
            continue
        img = shots[crop["screen"]]
        k = img.width / APP["width"]
        x, y, w, h = crop["box"]
        cut = img.crop((round(x * k), round(y * k), round((x + w) * k), round((y + h) * k)))
        save(cut.resize((round(w * 2), round(h * 2)), Image.LANCZOS), crop_id)  # crops ship at 2x
    if tray_img:
        save(tray_img, "tray")


def capture_tray(browser) -> Image.Image:
    tray = SPEC["tray"]
    page = browser.new_page(viewport={"width": tray["width"], "height": tray["height"]}, device_scale_factor=tray["dpr"])
    page.clock.set_fixed_time(CLOCK)
    page.goto(f"{BASE}/demo/tray?active=kestrel", wait_until="networkidle")
    check(page, "tray", ["Accounts", "Kestrel", "Vesper"], [])
    time.sleep(2.5)
    tray_img = Image.open(io.BytesIO(page.screenshot())).convert("RGB")
    browser.close()
    return tray_img


if __name__ == "__main__":
    main()
