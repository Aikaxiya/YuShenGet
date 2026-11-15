# -*- coding: utf-8 -*-
"""按官方资料重建 / 补齐 characters.json 的角色名录与头像 base64。

用法：
    python tools/gen_avatars.py                         # 按现有配置补齐缺失头像
    python tools/gen_avatars.py --force                 # 全部重新下载头像
    python tools/gen_avatars.py --rebuild-roster        # 先按官方名录重建 characters 段，再补头像
    python tools/gen_avatars.py --rebuild-roster --force [配置文件路径]

数据来源（都是官方 / 游戏内资源，不使用第三方角色数据库）：
- 角色名录、元素、游戏内部图标名：Enka 维护的游戏内角色表
  （store/characters.json + store/loc.json 的 zh-cn），经 fastly.jsdelivr.net 镜像获取。
- 该表只覆盖到「月之三」，之后的新角色按米游社官方《全角色立绘》（5.4~7.3）
  与官网版本更新说明（7.0 / 7.1 / 7.2）补录在 EXTRA_CHARACTERS。
- 头像图片：优先米哈游官方图床，官方图床只收录到 3.x，4.0 之后统一从
  游戏内同款资源 `enka.network/ui` 取。**只在本脚本生成阶段访问**，
  结果直接内嵌为 base64，运行时不发任何请求，avatar_sources 仍保持官方地址模板。

生成的 `avatars` 段形如：
    "avatars": { "<角色名>": "data:image/png;base64,..." }
"""

from __future__ import annotations

import base64
import io
import json
import sys
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_CONFIG = ROOT / "target" / "release" / "data" / "characters.json"
SIZE = 64
USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) TeyvatGacha/1.0"
HEADERS = {"User-Agent": USER_AGENT, "Referer": "https://webstatic.mihoyo.com/"}

# 运行时配置里的地址模板：保持米哈游官方图床，用户可自行替换。
OFFICIAL_SOURCES = [
    "https://upload-os-bbs.mihoyo.com/game_record/genshin/character_icon/UI_AvatarIcon_{icon}.png",
    "https://upload-os-bbs.hoyolab.com/game_record/genshin/character_icon/UI_AvatarIcon_{icon}.png",
]
# 只在生成阶段补齐官方图床缺失的 4.0+ 角色，绝不写进运行时 avatar_sources。
MIRROR_SOURCES = ["https://enka.network/ui/UI_AvatarIcon_{icon}.png"]

ENKA_CHARACTERS_URL = (
    "https://fastly.jsdelivr.net/gh/EnkaNetwork/API-docs@master/store/characters.json"
)
ENKA_LOC_URL = "https://fastly.jsdelivr.net/gh/EnkaNetwork/API-docs@master/store/loc.json"

ELEMENT_MAP = {
    "Wind": "anemo",
    "Ice": "cryo",
    "Electric": "electro",
    "Fire": "pyro",
    "Water": "hydro",
    "Grass": "dendro",
    "Rock": "geo",
}

# 米游社官方《全角色立绘》(5.4~7.3) 与官网 7.0/7.1/7.2 更新说明补录。
# (简体中文名, 元素, 游戏内部图标名)；元素留空表示官方资料未明确，用 unknown 兜底，
# 图标名留空表示游戏资源尚未确认，配置里先占位，等下个版本补上即可。
EXTRA_CHARACTERS = [
    # 「月之四」及之后
    ("哥伦比娅", "hydro", "Columbina"),  # 挪德卡莱的月神「库塔尔」
    ("兹白", "geo", "Zibai"),
    ("叶洛亚", "", ""),
    ("法尔伽", "anemo", "Varka"),
    ("莉奈娅", "", ""),
    ("尼可", "", "Nicole"),
    ("洛恩", "", ""),
    ("布伦妮", "", ""),
    ("桑多涅", "", ""),
    # 7.0「无神怜爱的雪国」
    ("奥黛塔", "cryo", "Odette"),
    ("阿罗夏", "", "Alyosha"),
    # 7.1「往冥府的安魂歌」：官网明确「薇斯纳(风)」「沃雅妮莎(水)」
    ("薇斯纳", "anemo", "Vesna"),
    ("沃雅妮莎", "hydro", ""),
    # 7.2「坏灭肇始之时」：官网明确「米提亚(雷·法器)」「瓦列里(雷·单手剑)」
    ("米提亚", "electro", "Mitya"),
    ("瓦列里", "electro", "Valeriy"),
    # 7.3（未上线，仅预留名录）
    ("安娜丝塔夏", "", ""),
    ("达妮卡", "", "Danica"),
]


SEED_HEADER = """// 提瓦特祈愿分析 · 角色配置（文件仍叫 .json，但允许 // 与 /* */ 注释）
// ------------------------------------------------------------------
// 新增角色：在 "characters" 段加一条，例如
//   "新角色": { "element": "anemo", "icon": "游戏内部英文名" }
// 元素：anemo / cryo / electro / pyro / hydro / dendro / geo / unknown
// icon：游戏内部图标名，用来拼 UI_AvatarIcon_{icon}.png（不是简体中文名）
// 头像：写进 "avatars" 段的 base64，或留空让程序按 avatar_sources 自动下载
// 自定义条目建议放 characters.local.json，程序自动合并且不改写本文件
// 重建名录 / 头像：python tools/gen_avatars.py --rebuild-roster --emit-seed
// ------------------------------------------------------------------
"""


def strip_json_comments(text: str) -> str:
    """剥掉 // 与 /* */ 注释；字符串内部的 //（如 https://、base64）原样保留。"""
    out: list[str] = []
    in_string = escaped = False
    i, n = 0, len(text)
    while i < n:
        ch = text[i]
        nxt = text[i + 1] if i + 1 < n else ""
        if in_string:
            out.append(ch)
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == '"':
                in_string = False
            i += 1
            continue
        if ch == '"':
            in_string = True
            out.append(ch)
            i += 1
            continue
        if ch == "/" and nxt == "/":
            i += 2
            while i < n and text[i] != "\n":
                i += 1
            continue
        if ch == "/" and nxt == "*":
            i += 2
            while i + 1 < n and not (text[i] == "*" and text[i + 1] == "/"):
                i += 1
            i += 2
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def leading_comment_block(text: str) -> str:
    """取文件开头的注释块，重建配置时原样保留，不冲掉用户写的说明。"""
    kept: list[str] = []
    for line in text.splitlines(keepends=True):
        stripped = line.strip()
        if not stripped or stripped.startswith(("//", "/*", "*")):
            kept.append(line)
        else:
            break
    return "".join(kept)


def fetch_json(url: str) -> dict:
    request = urllib.request.Request(url, headers=HEADERS)
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.loads(response.read())


def build_roster() -> dict[str, dict[str, str]]:
    """从游戏内角色表生成「中文名 → 元素 + 图标名」，再合并官方公告补录的新角色。"""
    characters = fetch_json(ENKA_CHARACTERS_URL)
    loc = fetch_json(ENKA_LOC_URL).get("zh-cn", {})
    roster: dict[str, dict[str, str]] = {}

    for key, item in characters.items():
        base_id = int(key.split("-")[0])
        # 100009xx 是试用角色、110000xx 是测试角色；带 '-' 的是旅行者元素变体
        if base_id >= 10000900 or "-" in key:
            continue
        icon = (item.get("SideIconName") or "").replace("UI_AvatarIcon_Side_", "")
        name = loc.get(str(item.get("NameTextMapHash", "")), "")
        if not icon or not name or name == "?":
            continue
        roster[name] = {
            "element": ELEMENT_MAP.get(item.get("Element"), "unknown"),
            "icon": icon,
        }

    for name, element, icon in EXTRA_CHARACTERS:
        roster.setdefault(
            name, {"element": element or "unknown", "icon": icon}
        )
    return roster


def download(template: str, icon: str) -> bytes | None:
    url = template.replace("{icon}", icon)
    request = urllib.request.Request(url, headers=HEADERS)
    for attempt in range(2):
        try:
            with urllib.request.urlopen(request, timeout=12) as response:
                if response.status != 200:
                    return None
                data = response.read()
                return data if len(data) > 512 else None
        except (urllib.error.HTTPError, ValueError):
            return None
        except (urllib.error.URLError, OSError, TimeoutError):
            if attempt == 1:
                return None
    return None


def to_data_url(raw: bytes) -> str | None:
    try:
        image = Image.open(io.BytesIO(raw)).convert("RGBA")
    except Exception as exc:  # noqa: BLE001 - 损坏图片直接跳过
        print(f"  ! 图片无法解析：{exc}")
        return None
    image = image.resize((SIZE, SIZE), Image.LANCZOS)
    buffer = io.BytesIO()
    image.save(buffer, format="PNG", optimize=True)
    return "data:image/png;base64," + base64.b64encode(buffer.getvalue()).decode("ascii")


MIRROR_ACTIVE = True  # --official-only 时由 main 关掉


def fetch_avatar(task: tuple[str, str]) -> tuple[str, str | None, str]:
    name, icon = task
    for template in OFFICIAL_SOURCES:
        raw = download(template, icon)
        if raw is not None:
            return name, to_data_url(raw), "官方"
    if MIRROR_ACTIVE:
        for template in MIRROR_SOURCES:
            raw = download(template, icon)
            if raw is not None:
                return name, to_data_url(raw), "游戏内资源"
    return name, None, "失败"


def main() -> int:
    flags = [item for item in sys.argv[1:] if item.startswith("--")]
    plain_args = [item for item in sys.argv[1:] if not item.startswith("--")]
    force = "--force" in flags
    rebuild = "--rebuild-roster" in flags
    official_only = "--official-only" in flags
    path = Path(plain_args[0]) if plain_args else DEFAULT_CONFIG

    if path.exists():
        original = path.read_text(encoding="utf-8")
        config = json.loads(strip_json_comments(original))
    else:
        original = ""
        config = {}

    if rebuild:
        try:
            roster = build_roster()
        except Exception as exc:  # noqa: BLE001
            print(f"官方角色表获取失败：{exc}")
            return 1
        config["characters"] = roster
        print(f"名录重建：{len(roster)} 名角色（含官方公告补录 {len(EXTRA_CHARACTERS)} 名）")
        if force:
            config["avatars"] = {}
    else:
        config.setdefault("characters", {})

    config.setdefault("avatar_sources", list(OFFICIAL_SOURCES))
    avatars = config.setdefault("avatars", {})
    characters = config["characters"]

    targets = [
        (name, entry.get("icon", ""))
        for name, entry in characters.items()
        if entry.get("icon")
        and (force or not avatars.get(name, "").startswith("data:image/"))
    ]
    print(f"需要取图 {len(targets)} 张（已有 {len(avatars)} 张内嵌头像）")

    ok = failed = mirrored = 0
    failures: list[str] = []
    if targets:
        global MIRROR_ACTIVE
        MIRROR_ACTIVE = not official_only
        with ThreadPoolExecutor(max_workers=12) as pool:
            results = list(pool.map(fetch_avatar, targets))
        for name, data_url, origin in results:
            if data_url is None:
                failed += 1
                failures.append(name)
                continue
            avatars[name] = data_url
            ok += 1
            if origin == "游戏内资源":
                mirrored += 1
        print(f"下载完成：成功 {ok}（游戏内资源补齐 {mirrored}），失败 {failed}")

    header = leading_comment_block(original) or SEED_HEADER
    if header and not header.endswith("\n"):
        header += "\n"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        header + json.dumps(config, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    print(f"配置文件：{path} （{path.stat().st_size / 1024:.1f} KB，名录 {len(characters)}，头像 {len(avatars)}）")

    # 同步生成「默认名录种子」：不带 base64，随 exe 分发，首次启动时写成本地配置。
    if "--emit-seed" in flags:
        seed_path = ROOT / "apps" / "desktop" / "src-tauri" / "resources" / "characters.seed.json"
        seed_path.parent.mkdir(parents=True, exist_ok=True)
        seed = {
            "avatar_sources": config["avatar_sources"],
            "characters": config["characters"],
        }
        seed_path.write_text(
            SEED_HEADER + json.dumps(seed, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        print(f"默认名录种子：{seed_path} （{len(characters)} 名角色，不含头像）")

        # 安装包资源：带头像的完整配置，装完直接就是安装目录下的 data/characters.json
        bundle_path = seed_path.parent / "characters.json"
        bundle_path.write_text(
            header + json.dumps(config, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        print(f"安装包资源：{bundle_path} （含 {len(avatars)} 张头像）")

    if failures:
        print("未取到头像（图标名待确认或资源未上线）：" + "、".join(failures))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
