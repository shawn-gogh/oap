#!/usr/bin/env node
// Extractor / coverage report for the localization codemod.
//
//   node i18n/extract.cjs          # write candidates.json + print coverage
//
// Walks every .ts/.tsx under src/, collects the same translatable units the
// loader acts on, keeps the ones that look like natural-language UI copy, and
// writes i18n/candidates.json (key -> ["file:line", ...]). It also prints how
// many candidates are still missing from zh.json so you can see coverage and
// catch new upstream strings after a merge.

const fs = require("fs");
const path = require("path");
const { collectUnits } = require("./collect.cjs");

const UI_DIR = path.join(__dirname, "..");
const SRC_DIR = path.join(UI_DIR, "src");
const DICT_PATH = path.join(__dirname, "zh.json");
const OUT_PATH = path.join(__dirname, "candidates.json");

function walk(dir, acc) {
  for (const name of fs.readdirSync(dir)) {
    const full = path.join(dir, name);
    const stat = fs.statSync(full);
    if (stat.isDirectory()) {
      if (name === "node_modules" || name === ".next") continue;
      walk(full, acc);
    } else if (/\.(tsx|ts)$/.test(name) && !/\.d\.ts$/.test(name)) {
      acc.push(full);
    }
  }
  return acc;
}

// Keep only strings that read like human-facing copy, to keep the working
// sheet clean. The dictionary is the source of truth either way.
function isUiCopy(text, kind) {
  if (!/[A-Za-z]/.test(text)) return false;
  if (kind === "jsx-text") return true;
  const t = text.trim();
  if (t.length < 2) return false;
  if (/^https?:\/\//.test(t)) return false; // urls
  if (/^[/.#@]/.test(t)) return false; // paths, selectors, handles
  if (/^[a-z0-9_]+$/.test(t)) return false; // single lowercase identifier
  if (/^[a-z][a-z0-9]*([:-][a-z0-9]+)+(\s+[a-z][a-z0-9:-]*)*$/.test(t)) return false; // tailwind-ish class lists
  // Require either a space, a leading capital, or sentence punctuation.
  if (/\s/.test(t) || /^[A-Z]/.test(t) || /[.!?…:]$/.test(t)) return true;
  return false;
}

function lineOf(source, offset) {
  let line = 1;
  for (let i = 0; i < offset && i < source.length; i++) {
    if (source[i] === "\n") line++;
  }
  return line;
}

function main() {
  let dict = {};
  try {
    dict = JSON.parse(fs.readFileSync(DICT_PATH, "utf8"));
  } catch {
    dict = {};
  }

  const files = walk(SRC_DIR, []);
  const candidates = {}; // key -> Set(refs)

  for (const file of files) {
    const source = fs.readFileSync(file, "utf8");
    let units;
    try {
      units = collectUnits(source, file);
    } catch (e) {
      console.error(`parse failed: ${file}: ${e.message}`);
      continue;
    }
    const rel = path.relative(UI_DIR, file);
    for (const u of units) {
      if (!isUiCopy(u.text, u.kind)) continue;
      const ref = `${rel}:${lineOf(source, u.start)}`;
      (candidates[u.text] ||= new Set()).add(ref);
    }
  }

  const keys = Object.keys(candidates).sort((a, b) => a.localeCompare(b));
  const out = {};
  for (const k of keys) out[k] = [...candidates[k]].sort();
  fs.writeFileSync(OUT_PATH, JSON.stringify(out, null, 2) + "\n");

  const missing = keys.filter((k) => !(k in dict) || dict[k] === "");
  console.log(`files scanned : ${files.length}`);
  console.log(`candidates    : ${keys.length}`);
  console.log(`translated    : ${keys.length - missing.length}`);
  console.log(`missing       : ${missing.length}`);
  if (missing.length) {
    console.log(`\n--- missing (not yet in zh.json) ---`);
    for (const k of missing) console.log(JSON.stringify(k));
  }
  console.log(`\nwrote ${path.relative(UI_DIR, OUT_PATH)}`);
}

main();
