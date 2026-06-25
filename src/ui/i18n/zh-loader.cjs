// Build-time localization webpack loader.
//
// Runs before Next's SWC loader on our own source files. For every translatable
// string whose English text has an entry in zh.json, we splice the Chinese
// translation into the source at the exact byte span the collector reported.
// Strings with no dictionary entry are left untouched (so the UI degrades to
// English, never to a broken/blank label).
//
// The .tsx/.ts files on disk stay byte-identical to upstream; the only artifact
// we maintain is zh.json. That keeps the merge-conflict surface with the
// open-source upstream essentially zero.

const fs = require("fs");
const path = require("path");
const { collectUnits } = require("./collect.cjs");

const DICT_PATH = path.join(__dirname, "zh.json");

function loadDict() {
  try {
    return JSON.parse(fs.readFileSync(DICT_PATH, "utf8"));
  } catch {
    return {};
  }
}

// JSX text can't contain raw { } < >. If a translation does, emit it as a
// JSX expression container instead of bare text.
function renderJsxText(zh) {
  if (/[{}<>]/.test(zh)) {
    return `{${JSON.stringify(zh)}}`;
  }
  return zh;
}

function renderTemplate(zh) {
  return "`" + zh.replace(/\\/g, "\\\\").replace(/`/g, "\\`").replace(/\$\{/g, "\\${") + "`";
}

module.exports = function zhLoader(source) {
  const dict = loadDict();
  this.addDependency(DICT_PATH);

  // Cheap bail-out: nothing to do if no dictionary key appears verbatim.
  if (Object.keys(dict).length === 0) return source;

  let units;
  try {
    units = collectUnits(source, this.resourcePath);
  } catch {
    // Never let a parse hiccup break the build; fall back to English.
    return source;
  }

  // Replace from the end so earlier byte offsets stay valid.
  const edits = [];
  for (const u of units) {
    const zh = dict[u.text];
    if (zh == null || zh === "" || zh === u.text) continue;
    let replacement;
    if (u.kind === "jsx-text") replacement = renderJsxText(zh);
    else if (u.kind === "template") replacement = renderTemplate(zh);
    else replacement = JSON.stringify(zh);
    edits.push({ start: u.start, end: u.end, replacement });
  }

  if (edits.length === 0) return source;
  edits.sort((a, b) => b.start - a.start);

  let out = source;
  for (const e of edits) {
    out = out.slice(0, e.start) + e.replacement + out.slice(e.end);
  }
  return out;
};
