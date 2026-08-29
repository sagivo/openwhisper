"""Generate a 1024x1024 macOS app icon source from the brand emoji.

Produces src-tauri/icons/app-icon-source.png. Run `npx tauri icon
src-tauri/icons/app-icon-source.png` afterward to generate the full
.icns / .ico / mip-mapped png set.

Delegates to scripts/gen_emoji_icons.swift so Apple Color Emoji renders natively.
"""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
SWIFT = ROOT / "scripts" / "gen_emoji_icons.swift"


def main() -> None:
    subprocess.check_call(["swift", str(SWIFT), str(ROOT)])
    print("next: npx tauri icon src-tauri/icons/app-icon-source.png", file=sys.stderr)


if __name__ == "__main__":
    main()
