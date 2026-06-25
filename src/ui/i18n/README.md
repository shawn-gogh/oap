# 中文本地化（构建期 codemod）

UI 的中文汉化通过**构建期 AST 代码改写**实现，源码 `.ts`/`.tsx` 保持英文原样不动，
只维护一份词典。这样与上游开源仓库 merge 时几乎不会产生冲突。

## 组成

| 文件 | 作用 |
| --- | --- |
| `zh.json` | 词典：`{ 英文原文: 中文译文 }`。**唯一需要人工维护的文件。** |
| `collect.cjs` | 用 TypeScript 编译器 API 采集可翻译节点（JSX 文本、字符串字面量、模板字面量），返回精确字节区间。loader 与 extractor 共用，口径一致。 |
| `zh-loader.cjs` | webpack `pre`-loader：对命中 `zh.json` 的节点按字节区间原地替换为中文，再交给 SWC 编译。未命中的字符串保持英文（永不出现空白/损坏）。 |
| `extract.cjs` | 扫描 `src/`，输出候选串到 `candidates.json`，并打印覆盖率与缺失清单。 |

接入点：`next.config.mjs` 的 `webpack()` 把 loader 以 `enforce: "pre"` 挂到 `src/**` 上。
`package.json` 的 build 固定为 `next build --webpack`（Next 16 默认 Turbopack 会拒绝 webpack 配置）。

## 安全边界（不会改坏逻辑）

只替换“命中词典 key”的节点，因此：
- enum/状态值（如 `"queued"`、`"idle"`）、`"use client"` 指令、CSS 类名、SVG 路径、品牌名、环境变量名等**都不在词典里**，自然不被触碰。
- 采集器还显式跳过 import 路径、类型字面量、对象键名、以及 `className`/`d` 等非文案属性。

## 日常维护

```bash
# 看覆盖率 + 列出尚未翻译的新增英文串（上游合并后常用）
node i18n/extract.cjs

# 翻译：把缺失的英文串作为 key 加进 zh.json，填上中文即可
```

上游改了界面文案后，旧 key 失配会自动回退英文、`extract.cjs` 会把新串列入缺失清单——
补进 `zh.json` 即可，无需改任何源码。

> 后端面向用户的报错文案在 `src/i18n.rs`（`GatewayError` 的集中翻译）。
