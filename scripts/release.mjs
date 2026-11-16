#!/usr/bin/env node
// 发一个新版本：改版本号、提交、打标签。推送标签后 GitHub 会自动打包并发布 Release。
//
//   npm run release -- 0.2.0
//   git push --follow-tags

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

const version = (process.argv[2] ?? "").replace(/^v/, "");
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error("用法: npm run release -- <版本号>，例如 0.2.0 或 0.2.0-beta.1");
  process.exit(1);
}
const tag = `v${version}`;

const run = (cmd, args, opts = {}) =>
  // 不捕获输出时 execFileSync 返回 null
  (execFileSync(cmd, args, { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"], ...opts }) ?? "").trim();

if (run("git", ["status", "--porcelain"]) !== "") {
  console.error("工作区还有没提交的改动，先提交或暂存再发版。");
  process.exit(1);
}
if (run("git", ["tag", "--list", tag]) !== "") {
  console.error(`标签 ${tag} 已经存在。`);
  process.exit(1);
}

// package.json 是应用版本号的唯一来源，tauri.conf.json 直接引用它
const pkg = JSON.parse(readFileSync("package.json", "utf8"));
const previous = pkg.version;
pkg.version = version;
writeFileSync("package.json", `${JSON.stringify(pkg, null, 2)}\n`);

const lock = JSON.parse(readFileSync("package-lock.json", "utf8"));
lock.version = version;
if (lock.packages?.[""]) lock.packages[""].version = version;
writeFileSync("package-lock.json", `${JSON.stringify(lock, null, 2)}\n`);

// Rust 各个 crate 的版本跟着一起改，免得 --version 之类的地方对不上
const cargo = readFileSync("Cargo.toml", "utf8");
const updated = cargo.replace(
  /(\[workspace\.package\][^[]*?\nversion = ")[^"]*(")/,
  `$1${version}$2`,
);
if (updated === cargo && !cargo.includes(`version = "${version}"`)) {
  console.error("没能在 Cargo.toml 的 [workspace.package] 里找到 version。");
  process.exit(1);
}
writeFileSync("Cargo.toml", updated);
run("cargo", ["update", "--workspace", "--offline"], { stdio: ["ignore", "ignore", "inherit"] });

run("git", ["add", "package.json", "package-lock.json", "Cargo.toml", "Cargo.lock"]);
run("git", ["commit", "-m", `发布 ${tag}`]);
run("git", ["tag", "-a", tag, "-m", `Postflow ${tag}`]);

console.log(`版本 ${previous} → ${version}，已提交并打上标签 ${tag}。`);
console.log("推送后 GitHub 会自动打包三个平台的安装包并发布 Release：");
console.log("");
console.log("  git push --follow-tags");
