#!/usr/bin/env python3
"""Bundle src/ into one self-contained page for the web preview.

The desktop app loads src/index.html directly; the preview needs every
stylesheet and script inlined. Output: preview/index.html (page content
only, no <html>/<head>/<body> wrapper, as the preview host adds its own).
"""
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "src"
OUT = ROOT / "preview" / "index.html"

html = (SRC / "index.html").read_text(encoding="utf-8")
styles = re.findall(r'<link rel="stylesheet" href="([^"]+)">', html)
scripts = re.findall(r'<script src="([^"]+)"></script>', html)
body = re.search(r"<body>(.*?)<script src=", html, re.S).group(1).strip()
import base64
def inline_img(m):
    data = base64.b64encode((SRC / m.group(1)).read_bytes()).decode()
    return f'src="data:image/png;base64,{data}"'
body = re.sub(r'src="(assets/[^"]+\.png)"', inline_img, body)

css = "\n".join((SRC / s).read_text(encoding="utf-8") for s in styles)
js = "\n".join((SRC / s).read_text(encoding="utf-8").replace("</script", "<\\/script") for s in scripts)
# the agent skill, so the preview can show / copy it (the app reads it from the binary)
import json
skill = (ROOT / "skills" / "shiyi" / "SKILL.md").read_text(encoding="utf-8")
js = "window.__SHIYI_SKILL__ = " + json.dumps(skill, ensure_ascii=False).replace("</", "<\\/") + ";\n" + js
# model provider logos (the app loads them from assets/providers; the single-file preview can't)
icons = {f.stem: "data:image/svg+xml;base64," + base64.b64encode(f.read_bytes()).decode() for f in sorted((SRC / "assets" / "providers").glob("*.svg"))}
js = "window.__SHIYI_PROVIDER_ICONS__ = " + json.dumps(icons) + ";\n" + js

page = f"""<title>拾忆</title>
<style>
{css}
</style>
{body}
<script>
{js}
</script>
"""
OUT.parent.mkdir(exist_ok=True)
OUT.write_text(page, encoding="utf-8")
print(f"wrote {OUT.relative_to(ROOT)} ({len(page) // 1024} KB) from {len(styles)} stylesheets, {len(scripts)} scripts")
