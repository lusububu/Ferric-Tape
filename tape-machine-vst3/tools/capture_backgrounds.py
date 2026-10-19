"""Capture each panel's background wash (assets/backgrounds/*.png) straight out of the working
web app's HTML, using headless Chromium via Playwright, for editor.rs to embed with
`include_bytes!`. Run this again whenever the source HTML's palette, gradient stops, or panel
layout changes - nothing in editor.rs derives these images, they're exact pixel data from a real
browser render.

Usage:
    python3 tools/capture_backgrounds.py path/to/Them_Changes_Tape_Machine.html

Why a screenshot instead of a procedurally generated texture: an earlier version of editor.rs
tried to re-derive the CSS radial-gradient() backgrounds by hand (per-pixel analytic falloff in
Rust). That approximation produced real, repeated bugs (visible banding, coverage gaps when a
panel's measured height outgrew the texture's guess). Screenshotting the real `.field` background
of each panel sidesteps that whole category of bug - what's captured is exactly what the browser
renders, not an approximation of it.

How each panel is captured:
  - Each panel's own foreground content (`.panel > .in`: headings, labels, knobs, dropdowns,
    buttons, meters, canvases) is set to `visibility:hidden` rather than `display:none`, so the
    panel keeps its true on-screen box (many of these panels get their height FROM that content -
    removing it from layout entirely collapses them, which was a real bug in an earlier version of
    this script).
  - The page-wide grain overlay (`body::after`) is disabled during capture: in the real CSS, grain
    sits above every panel AND its foreground content as a single page-wide layer, not something
    that belongs to any one panel's background. editor.rs reproduces that overlay separately
    (`paint_grain_tiled`, procedural, not from a PNG) rather than baking grain per panel here.
  - Each panel is screenshotted clipped to its own real bounding box, so what's captured is exactly
    what's visible on screen (the browser's own `overflow:hidden` on `.panel` already clips
    whatever the oversized `.field` div bleeds past the panel edge).
"""
import pathlib
import sys

from playwright.sync_api import sync_playwright

OUT_DIR = pathlib.Path(__file__).resolve().parent.parent / "assets" / "backgrounds"

# (output filename, CSS selector for the .panel element)
TARGETS = [
    ("masthead.png", "header.panel.mast"),
    ("deck.png", "section.panel.deck"),
    ("scope.png", '[aria-label="Hysteresis scope"]'),
    ("response.png", '[aria-label="Play-head response"]'),
    ("record.png", '[aria-label="Record"]'),
    ("playback.png", '[aria-label="Playback"]'),
    ("transport.png", '[aria-label="Transport"]'),
]


def main():
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} path/to/Them_Changes_Tape_Machine.html", file=sys.stderr)
        sys.exit(1)
    html_path = pathlib.Path(sys.argv[1]).resolve()
    if not html_path.is_file():
        print(f"not a file: {html_path}", file=sys.stderr)
        sys.exit(1)
    src_url = html_path.as_uri()

    OUT_DIR.mkdir(parents=True, exist_ok=True)

    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(viewport={"width": 1200, "height": 1600}, device_scale_factor=2)
        page.goto(src_url)
        page.wait_for_timeout(500)  # let fonts, JS content population, and the grain canvas settle
        page.add_style_tag(content=".panel>.in{visibility:hidden!important} body::after{content:none!important}")

        for filename, selector in TARGETS:
            loc = page.locator(selector).first
            loc.wait_for(state="visible")
            box = loc.bounding_box()
            assert box, f"no bounding box for {selector}"
            page.screenshot(path=str(OUT_DIR / filename), clip=box)
            print(f"{filename}: {box['width']:.0f}x{box['height']:.0f} css px -> {OUT_DIR / filename}")

        browser.close()


if __name__ == "__main__":
    main()
