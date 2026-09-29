#!/usr/bin/env bash
# 打包 plugins/workbuddy 下的技能与专家为上架用 zip。
#
#   ./build.sh              # 打包全部
#   ./build.sh skill        # 打包两个技能
#   ./build.sh xhs-skill    # 只打小红书技能
#   ./build.sh social-skill # 只打多平台技能
#   ./build.sh expert       # 只打专家
#
# 专家包依赖技能包：专家 zip 会包含一份技能副本（XHS_SKILL）。
# 小红书技能源码的唯一位置是 plugins/workbuddy/xiaohongshu-socai，
# 专家目录下的 skills/ 是构建产物，不要手工编辑。

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DIST="$ROOT/dist"
XHS_SKILL_SRC="$ROOT/xiaohongshu-socai"
SOCIAL_SKILL_SRC="$ROOT/socai-social-research"
EXPERT_SRC="$ROOT/xiaohongshu-research-expert"

XHS_SKILL_NAME="xiaohongshu-socai"
SOCIAL_SKILL_NAME="socai-social-research"
EXPERT_NAME="xiaohongshu-research-expert"

TARGET="${1:-all}"

die() { echo "error: $*" >&2; exit 1; }

require_dir() { [ -d "$1" ] || die "缺少目录：$1"; }

zip_dir() {
  local src="$1" out="$2"
  rm -f "$out"
  ( cd "$(dirname "$src")" && zip -rq "$out" "$(basename "$src")" \
      -x "*.DS_Store" "*__pycache__*" "*.pyc" ".git/*" )
  printf '  %-38s %s\n' "$(basename "$out")" "$(du -h "$out" | cut -f1)"
}

check_frontmatter() {
  local file="$1"
  python3 - "$file" <<'PY'
import pathlib, re, sys

path = pathlib.Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
m = re.match(r"^---\n(.*?)\n---\n", text, re.S)
if not m:
    sys.exit(f"{path}: 缺少 YAML frontmatter")

lines = m.group(1).splitlines()
values = {}
index = 0
while index < len(lines):
    line = lines[index]
    match = re.fullmatch(r"([a-z][a-z0-9_-]*):(?:[ \t]*(.*))?", line)
    if not match:
        sys.exit(f"{path}: frontmatter 第 {index + 2} 行不是受支持的顶层字段")
    key, value = match.groups()
    if key in values:
        sys.exit(f"{path}: frontmatter 字段重复 {key}")
    if value in {">", "|"}:
        block = []
        index += 1
        while index < len(lines) and (not lines[index] or lines[index][0].isspace()):
            if lines[index]:
                if not lines[index].startswith("  "):
                    sys.exit(f"{path}: {key} 多行内容必须缩进两个空格")
                block.append(lines[index][2:])
            index += 1
        value = "\n".join(block).strip()
        values[key] = value
        continue
    values[key] = (value or "").strip()
    index += 1

required = {
    "name", "display_name", "display_name_en", "description", "description_zh",
    "description_en", "category", "version", "author", "allowed-tools",
}
missing = sorted(required - values.keys())
if missing:
    sys.exit(f"{path}: frontmatter 缺字段 {', '.join(missing)}")
empty = sorted(key for key in required if not values[key])
if empty:
    sys.exit(f"{path}: frontmatter 字段不能为空 {', '.join(empty)}")
if values["name"] != path.parent.name or not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", values["name"]):
    sys.exit(f"{path}: name 必须为目录名且使用小写 kebab-case")
semver = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-((?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)"
    r"(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$"
)
if not semver.fullmatch(values["version"]):
    sys.exit(f"{path}: version 必须为语义化版本")
if not re.fullmatch(r"[a-z][a-z0-9-]*", values["category"]):
    sys.exit(f"{path}: category 必须为小写标识符")
for key in ("display_name", "display_name_en"):
    if len(values[key]) > 64:
        sys.exit(f"{path}: {key} 超过 64 字符")
for key in ("description", "description_zh", "description_en"):
    if len(values[key]) > 1200:
        sys.exit(f"{path}: {key} 超过 1200 字符")
tools = [item.strip() for item in values["allowed-tools"].split(",") if item.strip()]
allowed_tools = {"Bash", "Read", "Write", "Glob", "Grep"}
unknown_tools = sorted(set(tools) - allowed_tools)
if not tools or len(tools) != len(set(tools)) or unknown_tools:
    sys.exit(f"{path}: allowed-tools 无效或重复 {', '.join(unknown_tools)}")
print(f"  frontmatter ok  {path.name}")
PY
}

check_skill_zip() {
  local archive="$1" name="$2"
  python3 - "$archive" "$name" <<'PY'
import pathlib, stat, sys, zipfile

archive = pathlib.Path(sys.argv[1])
name = sys.argv[2]
prefix = f"{name}/"
with zipfile.ZipFile(archive) as bundle:
    entries = bundle.infolist()
    names = {entry.filename for entry in entries}
    if f"{name}/SKILL.md" not in names:
        sys.exit(f"{archive}: 缺少 {name}/SKILL.md")
    for entry in entries:
        parts = pathlib.PurePosixPath(entry.filename).parts
        mode = entry.external_attr >> 16
        if (
            not entry.filename.startswith(prefix)
            or entry.filename.startswith("/")
            or ".." in parts
            or "\\" in entry.filename
            or stat.S_ISLNK(mode)
        ):
            sys.exit(f"{archive}: 不安全或越界条目 {entry.filename}")
    corrupt = bundle.testzip()
    if corrupt:
        sys.exit(f"{archive}: 压缩内容损坏 {corrupt}")
print(f"  archive ok      {archive.name}  entries={len(entries)}")
PY
}

check_plugin_json() {
  python3 - "$1" <<'PY'
import json, sys
path = sys.argv[1]
data = json.load(open(path, encoding="utf-8"))
required = [
    "name", "expertType", "version", "description", "author", "agents",
    "agentName", "displayName", "profession", "displayDescription",
    "avatar", "categoryId", "defaultInitPrompt", "plugin", "tags", "quickPrompts",
]
missing = [k for k in required if k not in data]
if missing:
    sys.exit(f"{path}: 缺必填字段 {', '.join(missing)}")
for field in ("displayName", "profession", "displayDescription", "defaultInitPrompt"):
    val = data[field]
    if not isinstance(val, dict) or not {"zh", "en"} <= set(val):
        sys.exit(f"{path}: {field} 必须同时含 zh 与 en")
zh_len = len(data["displayDescription"]["zh"])
if not 40 <= zh_len <= 50:
    sys.exit(f"{path}: displayDescription.zh 为 {zh_len} 字，规范要求 40-50 字")
if len(data["tags"]) != 3:
    sys.exit(f"{path}: tags 必须正好 3 个")
if len(data["quickPrompts"]) != 3:
    sys.exit(f"{path}: quickPrompts 必须正好 3 个")
if data["defaultInitPrompt"] != data["quickPrompts"][0]:
    sys.exit(f"{path}: defaultInitPrompt 必须与 quickPrompts 第一条一致")
if data["plugin"] != data["name"]:
    sys.exit(f"{path}: plugin 必须等于 name")
avatar = path.rsplit("/", 2)[0] + "/" + data["avatar"]
import os
if not os.path.exists(avatar):
    sys.exit(f"{path}: avatar 不存在 {data['avatar']}")
size = os.path.getsize(avatar)
if size > 512_000:
    sys.exit(f"{path}: 头像 {size} 字节，超过 500KB")
print(f"  plugin.json ok  category={data['categoryId']}  avatar={size // 1024}KB")
PY
}

build_skill() {
  local src="$1" name="$2"
  require_dir "$src"
  echo "技能 $name"
  check_frontmatter "$src/SKILL.md"
  zip_dir "$src" "$DIST/$name.zip"
  check_skill_zip "$DIST/$name.zip" "$name"
}

build_skills() {
  build_skill "$XHS_SKILL_SRC" "$XHS_SKILL_NAME"
  build_skill "$SOCIAL_SKILL_SRC" "$SOCIAL_SKILL_NAME"
}

build_expert() {
  require_dir "$EXPERT_SRC"
  require_dir "$XHS_SKILL_SRC"
  echo "专家 $EXPERT_NAME"

  check_plugin_json "$EXPERT_SRC/.codebuddy-plugin/plugin.json"

  # 组装暂存目录：专家源码 + 内联技能副本
  local stage="$DIST/.stage/$EXPERT_NAME"
  rm -rf "$DIST/.stage"
  mkdir -p "$stage/skills"
  ( cd "$EXPERT_SRC" && tar cf - --exclude=".DS_Store" --exclude="skills" . ) \
    | ( cd "$stage" && tar xf - )
  cp -R "$XHS_SKILL_SRC" "$stage/skills/$XHS_SKILL_NAME"

  zip_dir "$stage" "$DIST/$EXPERT_NAME.zip"
  rm -rf "$DIST/.stage"
}

mkdir -p "$DIST"

case "$TARGET" in
  skill)  build_skills ;;
  xhs-skill) build_skill "$XHS_SKILL_SRC" "$XHS_SKILL_NAME" ;;
  social-skill) build_skill "$SOCIAL_SKILL_SRC" "$SOCIAL_SKILL_NAME" ;;
  expert) build_expert ;;
  all)    build_skills; build_expert ;;
  *)      die "未知参数：$TARGET（可用：skill / xhs-skill / social-skill / expert / all）" ;;
esac

echo "完成 → $DIST"
