"""Check the opaque text and meaningful-graphic tokens used by the native UI."""
from pathlib import Path
import re

source = (Path(__file__).resolve().parents[1] / "src/ui/theme.rs").read_text(encoding="utf-8")
tokens = {name: tuple(map(int, channels)) for name, *channels in re.findall(
    r"const\s+(\w+):\s*Color32\s*=\s*Color32::from_rgb\((\d+),\s*(\d+),\s*(\d+)\)", source)}

def luminance(rgb):
    linear = [channel / 255 / 12.92 if channel / 255 <= .04045 else ((channel / 255 + .055) / 1.055) ** 2.4 for channel in rgb]
    return sum(value * weight for value, weight in zip(linear, [.2126, .7152, .0722]))

def ratio(foreground, background):
    light, dark = sorted([luminance(tokens[foreground]), luminance(tokens[background])], reverse=True)
    return (light + .05) / (dark + .05)

pairs = [(fg, bg, 4.5) for fg in ["INK", "MUTED", "ACCENT", "SUCCESS", "WARNING", "DANGER", "SNAPSHOT"] for bg in ["PAPER", "SURFACE", "SELECTED"]]
pairs += [("INK", "PRIMARY", 4.5), ("SIDEBAR", "DANGER", 4.5), ("CONNECTOR", "PAPER", 3), ("INPUT_BORDER", "SIDEBAR", 3)]
failed = []
for foreground, background, minimum in pairs:
    measured = ratio(foreground, background)
    print(f"{foreground:12} / {background:9}  {measured:.2f}:1  minimum {minimum}:1")
    if measured < minimum:
        failed.append(f"{foreground}/{background}: {measured:.2f}:1")
if failed:
    raise SystemExit("Failing pairs: " + ", ".join(failed))
print("All declared opaque token pairs pass. Antialiased glyphs and translucent surfaces require separate visual inspection.")
