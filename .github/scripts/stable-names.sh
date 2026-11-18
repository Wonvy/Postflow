#!/usr/bin/env bash
# 给 Release 里的每个安装包加一份不带版本号的副本，比如 Postflow-macOS.dmg。
# README 里的下载链接写成 releases/latest/download/Postflow-macOS.dmg，就一直指向最新版的安装包。
#
#   bash .github/scripts/stable-names.sh <release id>
#
# 需要 GH_TOKEN 和 GITHUB_REPOSITORY。已经有同名副本时先删掉旧的，所以可以重复运行。
set -euo pipefail

release_id="$1"
repo="$GITHUB_REPOSITORY"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

gh api "repos/$repo/releases/$release_id/assets" --paginate \
  --jq '.[] | [.id, .name] | @tsv' > "$work/assets.tsv"

# copy <版本号文件名的正则> <固定文件名>
copy() {
  local id source old
  id=$(PATTERN="$1" awk -F'\t' '$2 ~ ENVIRON["PATTERN"] && $2 !~ /\.sig$/ {print $1; exit}' "$work/assets.tsv")
  source=$(PATTERN="$1" awk -F'\t' '$2 ~ ENVIRON["PATTERN"] && $2 !~ /\.sig$/ {print $2; exit}' "$work/assets.tsv")
  if [ -z "$id" ]; then
    echo "::warning::Release 里没有匹配 $1 的安装包，跳过 $2"
    return
  fi
  old=$(NAME="$2" awk -F'\t' '$2 == ENVIRON["NAME"] {print $1}' "$work/assets.tsv")
  if [ -n "$old" ]; then
    gh api -X DELETE "repos/$repo/releases/assets/$old"
  fi
  gh api -H "Accept: application/octet-stream" "repos/$repo/releases/assets/$id" > "$work/$2"
  curl -sSf -X POST \
    -H "Authorization: Bearer $GH_TOKEN" -H "Content-Type: application/octet-stream" \
    --data-binary @"$work/$2" \
    "https://uploads.github.com/repos/$repo/releases/$release_id/assets?name=$2" > /dev/null
  rm "$work/$2"
  echo "$2 <- $source"
}

copy '_universal\.dmg$' Postflow-macOS.dmg
copy '_x64-setup\.exe$' Postflow-Windows-setup.exe
copy '_x64_en-US\.msi$' Postflow-Windows.msi
copy '_amd64\.AppImage$' Postflow-Linux.AppImage
copy '_amd64\.deb$' Postflow-Linux.deb
copy '\.x86_64\.rpm$' Postflow-Linux.rpm
