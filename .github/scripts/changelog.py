#!/usr/bin/env python3
"""从 CHANGELOG.md 里取更新说明，给发布流程用。

  changelog.py section 0.1.10   这个版本的说明，放在 GitHub 发布页最前面：英文，下面折叠着中文
  changelog.py recent 0.1.10    这个版本及之前几个版本的原文，放进 latest.json 的 notes，
                                应用的更新窗口按语言、按跳过了几个版本自己挑着显示

格式见 CHANGELOG.md 开头：每个版本一节「## 0.1.10 — 日期」，里面分「### English」和「### 中文」。
"""

import re
import sys

HEADING = re.compile(r"^## +v?(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)\b.*$", re.M)
# 放进 latest.json 的版本数，跳过好几个版本更新的人也能看到每一版改了什么
RECENT = 10


def sections(text):
    """[(版本号, 整节原文)]，按文件里的顺序，不含 Unreleased。"""
    marks = list(re.finditer(r"^## .*$", text, re.M))
    out = []
    for i, mark in enumerate(marks):
        m = HEADING.match(mark.group(0))
        if not m:
            continue
        end = marks[i + 1].start() if i + 1 < len(marks) else len(text)
        out.append((m.group(1), text[mark.start():end].strip()))
    return out


def parts(section):
    """一节里的英文和中文列表。没分语言时两边一样。"""
    body = section.split("\n", 1)[1] if "\n" in section else ""
    found = {}
    current = None
    for line in body.splitlines():
        sub = re.match(r"^### +(.+?)\s*$", line)
        if sub:
            name = sub.group(1).lower()
            current = "en" if name.startswith("english") else "zh" if name.startswith(("中文", "chinese")) else None
            continue
        found.setdefault(current, []).append(line)
    text = {k: "\n".join(v).strip() for k, v in found.items()}
    both = text.get(None, "")
    return text.get("en") or both, text.get("zh") or both


def key(version):
    core, _, pre = version.partition("-")
    return tuple(int(n) for n in core.split(".")), pre == "", pre


def main():
    command, version = sys.argv[1], sys.argv[2].lstrip("v")
    text = open("CHANGELOG.md", encoding="utf-8").read()
    found = sections(text)
    if command == "section":
        section = next((s for v, s in found if v == version), None)
        if section is None:
            print(f"::warning::CHANGELOG.md 里没有 {version} 的更新说明", file=sys.stderr)
            return
        en, zh = parts(section)
        print(en)
        if zh and zh != en:
            print("\n<details>\n<summary>中文更新说明</summary>\n\n" + zh + "\n\n</details>")
        print("\n---\n")
    elif command == "recent":
        upto = [s for v, s in found if key(v) <= key(version)]
        print("\n\n".join(upto[:RECENT]))
    else:
        sys.exit(f"unknown command: {command}")


if __name__ == "__main__":
    main()
