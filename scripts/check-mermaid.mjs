#!/usr/bin/env node
// check-mermaid.mjs — logos 文档 mermaid 块语法校验（防回归工具）
//
// 用途：扫描 logos/resources/（live 文档）下所有 .md 文件中的 ```mermaid 代码块，
// 逐块调用 mermaid.parse 做语法校验，输出 文件:行号 + 失败原因。
// 背景：fix-docs-mermaid-consistency 变更发现 S17 时序图因消息文本含 ASCII 分号
// 整图解析失败（; 是 mermaid 语句分隔符）。本脚本用于文档修改后的快速复查。
//
// 用法：
//   node scripts/check-mermaid.mjs            # 校验 logos/resources（live 文档）
//   node scripts/check-mermaid.mjs --all      # 连 logos/changes（含 archive 冻结记录）一起扫
//   node scripts/check-mermaid.mjs <file.md>  # 只校验指定文件
//
// 首次运行需安装依赖（仅在 scripts/mermaid-check/ 下，不污染主仓库）：
//   npm install --prefix scripts/mermaid-check
//
// 退出码：0 = 全部通过；1 = 存在语法失败块；2 = 依赖未安装/运行环境错误。

import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url)); // scripts/
const DEPS_DIR = path.join(HERE, 'mermaid-check');
const ROOT = path.resolve(HERE, '..');

const args = process.argv.slice(2);
const scanAll = args.includes('--all');
const explicitFile = args.find((a) => a.endsWith('.md'));

// ── 依赖加载（从 scripts/mermaid-check/node_modules 解析）─────────────────────
async function loadDeps() {
  const require = createRequire(path.join(DEPS_DIR, 'package.json'));
  try {
    const { JSDOM } = require('jsdom');
    const dom = new JSDOM('<!DOCTYPE html><html><body></body></html>');
    Object.defineProperty(globalThis, 'window', { value: dom.window });
    Object.defineProperty(globalThis, 'document', { value: dom.window.document });
    const mermaidEntry = pathToFileURL(require.resolve('mermaid')).href;
    const mermaid = (await import(mermaidEntry)).default;
    mermaid.initialize({ startOnLoad: false });
    return mermaid;
  } catch {
    console.error('依赖未安装。请先执行：');
    console.error('  npm install --prefix scripts/mermaid-check');
    process.exit(2);
  }
}

// ── 扫描 ────────────────────────────────────────────────────────────────────
function* walk(dir) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) yield* walk(p);
    else if (e.name.endsWith('.md')) yield p;
  }
}

function targets() {
  if (explicitFile) return [path.resolve(explicitFile)];
  const base = path.join(ROOT, 'logos');
  if (scanAll) return [...walk(base)];
  // live 文档：resources（changes/archive 是冻结历史记录，不在防回归范围）
  return [...walk(path.join(base, 'resources'))];
}

const mermaid = await loadDeps();

let total = 0;
let bad = 0;
for (const file of targets()) {
  const text = fs.readFileSync(file, 'utf8');
  const re = /```mermaid\n([\s\S]*?)```/g;
  let m;
  while ((m = re.exec(text))) {
    total++;
    const line = text.slice(0, m.index).split('\n').length;
    try {
      const r = await mermaid.parse(m[1]);
      if (r === false) throw new Error('parse returned false');
    } catch (err) {
      bad++;
      const msg = String(err.message || err).split('\n')[0];
      console.log(`FAIL ${path.relative(ROOT, file)}:${line}\n  ${msg}`);
    }
  }
}

console.log(`\n共 ${total} 块，语法失败 ${bad} 块`);
process.exit(bad === 0 ? 0 : 1);
