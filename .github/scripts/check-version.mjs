#!/usr/bin/env node
// 版本号一致性校验。
//
// 版本号在三个地方各写了一份，发版前必须一致：
//   - package.json
//   - src-tauri/tauri.conf.json
//   - src-tauri/Cargo.toml（[package] version）
//
// 用法：
//   node .github/scripts/check-version.mjs            # 只校验三处是否一致
//   node .github/scripts/check-version.mjs --tag v1.0.0  # 额外校验 tag 与版本号匹配
import { readFileSync } from "node:fs";

function fail(message) {
  console.error(`版本校验失败：${message}`);
  process.exit(1);
}

const pkgVersion = JSON.parse(readFileSync("package.json", "utf8")).version;
const tauriVersion = JSON.parse(
  readFileSync("src-tauri/tauri.conf.json", "utf8"),
).version;
const cargoVersion = readFileSync("src-tauri/Cargo.toml", "utf8").match(
  /^version\s*=\s*"([^"]+)"/m,
)?.[1];

if (!pkgVersion || !tauriVersion || !cargoVersion) {
  fail("无法从 package.json / tauri.conf.json / Cargo.toml 读取版本号");
}
if (
  pkgVersion !== tauriVersion ||
  pkgVersion !== cargoVersion
) {
  fail(
    `三处版本号不一致：package.json=${pkgVersion}，` +
      `tauri.conf.json=${tauriVersion}，Cargo.toml=${cargoVersion}`,
  );
}

const tagFlag = process.argv.indexOf("--tag");
if (tagFlag !== -1) {
  const tag = process.argv[tagFlag + 1] ?? "";
  // 允许 v1.0.0 与 1.0.0 两种写法，但 tag 必须带上正确的主版本号。
  if (tag.replace(/^v/, "") !== pkgVersion) {
    fail(`tag「${tag}」与版本号 ${pkgVersion} 不匹配`);
  }
}

console.log(
  `版本校验通过：${pkgVersion}${tagFlag !== -1 ? "（tag 匹配）" : ""}`,
);
