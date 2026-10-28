#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成内置主题包清单与三模式 Markdown 编辑器预览图。"""
import json
from pathlib import Path
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parents[1] / "themes"
THEMES = {
    "github": {
        "name": "GitHub", "author": "Aline", "accent": "#0969da",
        "modes": {
            "light": {"bg":"#ffffff","surface":"#ffffff","sidebar":"#f6f8fa","fg":"#1f2328","muted":"#59636e","border":"#d1d9e0","code":"#f6f8fa","accent":"#0969da","syntax":"#8250df"},
            "dark": {"bg":"#0d1117","surface":"#161b22","sidebar":"#0d1117","fg":"#f0f6fc","muted":"#9198a1","border":"#3d444d","code":"#161b22","accent":"#4493f8","syntax":"#d2a8ff"},
            "sepia": {"bg":"#f3efe4","surface":"#fbf8ef","sidebar":"#eee8da","fg":"#302f2b","muted":"#6b665b","border":"#ded7c8","code":"#eee8da","accent":"#956d32","syntax":"#8250df"},
        },
    },
    "notion": {
        "name": "Notion", "author": "Aline", "accent": "#2e67d1",
        "modes": {
            "light": {"bg":"#f7f7f5","surface":"#ffffff","sidebar":"#f1f1ef","fg":"#202124","muted":"#777873","border":"#e7e7e5","code":"#f1f1ef","accent":"#2e67d1","syntax":"#8b5cf6"},
            "dark": {"bg":"#191919","surface":"#202020","sidebar":"#191919","fg":"#e9e9e7","muted":"#a0a09d","border":"#373735","code":"#292927","accent":"#8ab4f8","syntax":"#c8a8ff"},
            "sepia": {"bg":"#f4f1e9","surface":"#fbf9f3","sidebar":"#efebe1","fg":"#39362f","muted":"#777166","border":"#e3ded2","code":"#efebe1","accent":"#9a6b3f","syntax":"#8b5cf6"},
        },
    },
    "vue": {
        "name": "Vue", "author": "Aline", "accent": "#42b883",
        "modes": {
            "light": {"bg":"#ffffff","surface":"#ffffff","sidebar":"#f6f8f7","fg":"#213547","muted":"#62717f","border":"#e2e8e7","code":"#f6f8f7","accent":"#42b883","syntax":"#8e5cd9"},
            "dark": {"bg":"#1b1b1f","surface":"#202127","sidebar":"#1b1b1f","fg":"#dfdfd6","muted":"#98989f","border":"#34363f","code":"#18181c","accent":"#42d392","syntax":"#c5a3ff"},
            "sepia": {"bg":"#f3f1e8","surface":"#fbfaf5","sidebar":"#efeee5","fg":"#303b37","muted":"#6e7974","border":"#dfdfd4","code":"#f0efe6","accent":"#32855f","syntax":"#764abc"},
        },
    },
    "feishu": {
        "name": "飞书", "author": "Aline", "accent": "#3370ff",
        "modes": {
            "light": {"bg":"#f3f5f8","surface":"#ffffff","sidebar":"#eef1f6","fg":"#1f2329","muted":"#646a73","border":"#e5e8ed","code":"#f5f7fa","accent":"#3370ff","syntax":"#7b61ff"},
            "dark": {"bg":"#1c1f24","surface":"#252a31","sidebar":"#1c1f24","fg":"#edf0f4","muted":"#a0a8b2","border":"#3b414a","code":"#20252b","accent":"#6b9cff","syntax":"#c3a6ff"},
            "sepia": {"bg":"#f3f0e8","surface":"#faf7ef","sidebar":"#eee9dd","fg":"#38352e","muted":"#70695d","border":"#e1dbce","code":"#eee9dd","accent":"#917243","syntax":"#7b61ff"},
        },
    },
}


def preview(theme, mode):
    c = theme["modes"][mode]
    esc = lambda value: escape(value, {'"': '&quot;'})
    family = theme["name"].lower()
    font = "Georgia,serif" if family == "notion" else "-apple-system,BlinkMacSystemFont,Segoe UI,sans-serif"
    top = c["accent"] if family == "vue" else c["surface"]
    top_text = "#ffffff" if family == "vue" else c["muted"]
    title = "开始使用 Vue" if family == "vue" else ("Project notes" if family == "notion" else "Markdown 样式指南")
    heading = "清晰的内容层级" if family == "vue" else "内容排版与层级"
    weight = "650" if family == "github" else "700"
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="960" height="540" viewBox="0 0 960 540" role="img" aria-label="{esc(theme['name'])} {mode} Markdown 样式预览">
<rect width="960" height="540" fill="{c['bg']}"/><rect width="960" height="38" fill="{top}"/><path d="M0 37.5H960" stroke="{c['border']}"/>
<circle cx="18" cy="19" r="5" fill="#ff6158"/><circle cx="35" cy="19" r="5" fill="#ffbd2f"/><circle cx="52" cy="19" r="5" fill="#28c840"/><text x="480" y="23" text-anchor="middle" fill="{top_text}" font-size="11" font-family="{font}">Aline  /  {esc(theme['name'])}</text>
<rect y="38" width="960" height="38" fill="{c['surface']}"/><path d="M0 75.5H960" stroke="{c['border']}"/><rect x="20" y="49" width="14" height="14" rx="3" fill="{c['accent']}" opacity=".15"/><path d="M24 53h6M24 57h6M24 60h4" stroke="{c['accent']}" stroke-width="1.2"/>
<text x="45" y="63" fill="{c['fg']}" font-size="11" font-weight="600" font-family="{font}">markdown-guide.md</text><text x="900" y="63" text-anchor="end" fill="{c['muted']}" font-size="10" font-family="{font}">已保存</text>
<rect y="76" width="176" height="464" fill="{c['sidebar']}"/><path d="M175.5 76V540" stroke="{c['border']}"/><text x="20" y="107" fill="{c['muted']}" font-size="10" font-weight="600" letter-spacing=".5" font-family="{font}">文档大纲</text>
<rect x="12" y="122" width="151" height="29" rx="4" fill="{c['accent']}" opacity=".1"/><rect x="12" y="122" width="2" height="29" rx="1" fill="{c['accent']}"/><text x="25" y="141" fill="{c['accent']}" font-size="11" font-weight="600" font-family="{font}">内容排版与层级</text><text x="25" y="173" fill="{c['muted']}" font-size="11" font-family="{font}">代码示例</text><text x="25" y="201" fill="{c['muted']}" font-size="11" font-family="{font}">对照表</text>
<rect x="210" y="96" width="720" height="420" rx="6" fill="{c['surface']}" stroke="{c['border']}"/><text x="258" y="129" fill="{c['muted']}" font-size="10" font-family="{font}">指南  /  MARKDOWN</text>
<text x="258" y="170" fill="{c['fg']}" font-size="27" font-weight="700" font-family="{font}">{esc(title)}</text><path d="M258 183H880" stroke="{c['border']}"/><text x="258" y="218" fill="{c['fg']}" font-size="17" font-weight="{weight}" font-family="{font}">{esc(heading)}</text><path d="M258 229H880" stroke="{c['border']}" opacity=".8"/>
<text x="258" y="254" fill="{c['fg']}" font-size="12" font-family="{font}">好的文档让结构一目了然，也让每段内容都更容易阅读。</text><text x="258" y="276" fill="{c['fg']}" font-size="12" font-family="{font}">使用 <tspan fill="{c['accent']}" font-family="SFMono-Regular,Menlo,monospace" font-size="11">清晰的层级</tspan> 组织信息，并为关键链接保留明确的强调色。</text><text x="258" y="301" fill="{c['accent']}" font-size="11" text-decoration="underline" font-family="{font}">阅读 Markdown 写作建议 →</text>
<rect x="258" y="319" width="622" height="93" rx="5" fill="{c['code']}" stroke="{c['border']}"/><path d="M293 329V402" stroke="{c['border']}"/><text x="277" y="345" text-anchor="middle" fill="{c['muted']}" font-size="10" font-family="SFMono-Regular,Menlo,monospace">1</text><text x="277" y="365" text-anchor="middle" fill="{c['muted']}" font-size="10" font-family="SFMono-Regular,Menlo,monospace">2</text><text x="277" y="385" text-anchor="middle" fill="{c['muted']}" font-size="10" font-family="SFMono-Regular,Menlo,monospace">3</text>
<text x="311" y="345" fill="{c['syntax']}" font-size="11" font-family="SFMono-Regular,Menlo,monospace">const</text><text x="352" y="345" fill="{c['fg']}" font-size="11" font-family="SFMono-Regular,Menlo,monospace">theme = </text><text x="406" y="345" fill="{c['accent']}" font-size="11" font-family="SFMono-Regular,Menlo,monospace">'calm'</text><text x="447" y="345" fill="{c['fg']}" font-size="11" font-family="SFMono-Regular,Menlo,monospace">;</text><text x="311" y="365" fill="{c['syntax']}" font-size="11" font-family="SFMono-Regular,Menlo,monospace">render</text><text x="352" y="365" fill="{c['fg']}" font-size="11" font-family="SFMono-Regular,Menlo,monospace">(document, </text><text x="424" y="365" fill="{c['accent']}" font-size="11" font-family="SFMono-Regular,Menlo,monospace">theme</text><text x="455" y="365" fill="{c['fg']}" font-size="11" font-family="SFMono-Regular,Menlo,monospace">);</text><text x="311" y="385" fill="{c['muted']}" font-size="11" font-family="SFMono-Regular,Menlo,monospace">// 保持内容专注</text>
<text x="258" y="445" fill="{c['fg']}" font-size="15" font-weight="650" font-family="{font}">对照表</text><rect x="258" y="458" width="622" height="39" rx="3" fill="{c['surface']}" stroke="{c['border']}"/><rect x="258" y="458" width="622" height="19" rx="3" fill="{c['code']}"/><path d="M258 477H880M498 458V497M710 458V497" stroke="{c['border']}"/>
<text x="270" y="472" fill="{c['muted']}" font-size="9" font-weight="600" font-family="{font}">元素</text><text x="510" y="472" fill="{c['muted']}" font-size="9" font-weight="600" font-family="{font}">样式</text><text x="722" y="472" fill="{c['muted']}" font-size="9" font-weight="600" font-family="{font}">用途</text><text x="270" y="491" fill="{c['fg']}" font-size="9" font-family="{font}">标题</text><text x="510" y="491" fill="{c['fg']}" font-size="9" font-family="{font}">层级清楚</text><text x="722" y="491" fill="{c['fg']}" font-size="9" font-family="{font}">组织内容</text>
</svg>'''

index = {"schemaVersion": 1, "themes": []}
for theme_id, theme in THEMES.items():
    folder = ROOT / theme_id
    previews = folder / "previews"
    previews.mkdir(parents=True, exist_ok=True)
    preview_paths = {}
    for mode in ("light", "dark", "sepia"):
        file = f"previews/{mode}.svg"
        (folder / file).write_text(preview(theme, mode), encoding="utf-8")
        preview_paths[mode] = file
    manifest = {"schemaVersion": 1, "id": theme_id, "name": theme["name"], "version": "1.0.0", "author": theme["author"], "builtin": True, "colors": theme["modes"], "previews": preview_paths}
    (folder / "theme.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    index["themes"].append({"id": theme_id, "manifest": f"{theme_id}/theme.json"})
(ROOT / "index.json").write_text(json.dumps(index, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
