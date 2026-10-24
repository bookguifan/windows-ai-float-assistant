"""Create deterministic, non-personal PNGs and reference transcripts for live OCR QA.

Requires Pillow and Microsoft YaHei. No AI-generated text or mock OCR results.
"""
import argparse
import hashlib
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", default=".test-artifacts/ocr-benchmark")
    parser.add_argument("--font", default="C:/Windows/Fonts/msyh.ttc")
    args = parser.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    font = ImageFont.truetype(args.font, 32)
    cases = [
        ("chinese", "plain", "截图识字验收\n春风拂过山岗，溪水流向远方。\n请保留标点、段落与换行。"),
        ("english", "plain", "OCR acceptance test\nThe quick brown fox jumps over the lazy dog.\nInvoice: AB-2026-1018; Total: 128.50 USD."),
        ("mixed", "plain", "中英混排 Test 42\n订单号：WB-20261018-A7\n易混字符：O0 I1 lL B8 S5\n路径：src/readback.js；比例：37.5%"),
        ("table", "table", "项目 数量 单价\n铅笔 12 2.50\n笔记本 3 18.00\n橡皮 5 1.20"),
    ]
    manifest = {
        "version": 1,
        "scope": "Four clear printed-text samples; not handwriting, scans or all-site OCR certification.",
        "font": args.font,
        "font_size": 32,
        "normalization": "NFC and remove whitespace only; table additionally unwraps Markdown table delimiters and separator rows.",
        "cases": [],
    }
    for name, kind, text in cases:
        lines = text.splitlines()
        image = Image.new("RGB", (1100, 80 + len(lines) * 65), "white")
        draw = ImageDraw.Draw(image)
        if kind == "table":
            xs = [35, 425, 655, 1065]
            ys = [35 + i * 65 for i in range(len(lines) + 1)]
            for x in xs:
                draw.line((x, ys[0], x, ys[-1]), fill="#777777", width=2)
            for y in ys:
                draw.line((xs[0], y, xs[-1], y), fill="#777777", width=2)
            for row, line in enumerate(lines):
                for col, cell in enumerate(line.split()):
                    draw.text((xs[col] + 18, ys[row] + 10), cell, font=font, fill="black")
        else:
            for i, line in enumerate(lines):
                draw.text((35, 25 + i * 65), line, font=font, fill="black")
        target = out / f"{name}.png"
        image.save(target)
        manifest["cases"].append({"id": name, "kind": kind, "file": target.name,
                                  "expected": text, "sha256": hashlib.sha256(target.read_bytes()).hexdigest()})
    (out / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"Generated {len(cases)} OCR benchmark images: {out.resolve()}")


if __name__ == "__main__":
    main()
