// Shared AST collector for the build-time localization codemod.
//
// Both the webpack loader (zh-loader.cjs) and the extractor (extract.cjs) use
// this so they agree exactly on what counts as a translatable string. We parse
// with the TypeScript compiler API (already a dependency) and return precise
// source spans, so the loader can splice translations in by byte offset without
// reprinting/reformatting the file. Source files stay byte-identical to
// upstream except for the spans we actually translate.

const ts = require("typescript");

// JSX attributes whose string values are markup/behavioral, never UI copy.
// Belt-and-suspenders: the dictionary only ever contains real phrases, but this
// stops a short word like a CSS class from ever being considered.
const SKIP_JSX_ATTRS = new Set([
  "className",
  "class",
  "id",
  "htmlFor",
  "type",
  "name",
  "href",
  "src",
  "rel",
  "target",
  "role",
  "key",
  "ref",
  "slot",
  "dir",
  "lang",
  "charSet",
  "style",
  "value",
  "defaultValue",
  "autoComplete",
  "inputMode",
  "enterKeyHint",
  "spellCheck",
  "pattern",
  "accept",
  "method",
  "action",
  "encType",
  "data-testid",
  "datatype",
  "itemProp",
  "property",
  "rev",
  "contentEditable",
  // SVG / geometry attributes — never UI copy.
  "d",
  "viewBox",
  "points",
  "transform",
  "fill",
  "stroke",
  "offset",
  "stopColor",
  "gradientTransform",
  "clipPath",
  "preserveAspectRatio",
  "xmlns",
]);

const KEYBOARD_EVENT_VALUES = new Set([
  "Enter",
  "Escape",
  "Tab",
  "ArrowUp",
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
  "Backspace",
  "Delete",
  "Space",
]);

function getEnclosingJsxAttrName(node) {
  let cur = node.parent;
  while (cur) {
    if (ts.isJsxAttribute(cur)) {
      return cur.name && cur.name.getText ? cur.name.getText() : String(cur.name && cur.name.escapedText);
    }
    // Stop climbing once we leave attribute territory.
    if (
      ts.isJsxElement(cur) ||
      ts.isJsxSelfClosingElement(cur) ||
      ts.isJsxFragment(cur) ||
      ts.isBlock(cur) ||
      ts.isArrowFunction(cur) ||
      ts.isFunctionDeclaration(cur)
    ) {
      return null;
    }
    cur = cur.parent;
  }
  return null;
}

function isModuleSpecifier(node) {
  const p = node.parent;
  if (!p) return false;
  return (
    (ts.isImportDeclaration(p) && p.moduleSpecifier === node) ||
    (ts.isExportDeclaration(p) && p.moduleSpecifier === node) ||
    (ts.isExternalModuleReference(p) && p.expression === node) ||
    (ts.isCallExpression(p) &&
      p.expression.kind === ts.SyntaxKind.ImportKeyword &&
      p.arguments[0] === node) ||
    (ts.isImportTypeNode && ts.isLiteralTypeNode(p))
  );
}

function isTypePosition(node) {
  const p = node.parent;
  return !!p && ts.isLiteralTypeNode(p);
}

function isDeclarationName(node) {
  const p = node.parent;
  if (!p) return false;
  // String literal used as a property/member *name* (key), not a value.
  if (
    (ts.isPropertyAssignment(p) ||
      ts.isPropertySignature(p) ||
      ts.isPropertyDeclaration(p) ||
      ts.isMethodSignature(p) ||
      ts.isMethodDeclaration(p) ||
      ts.isEnumMember(p) ||
      ts.isModuleDeclaration(p)) &&
    p.name === node
  ) {
    return true;
  }
  // obj["key"] element access is data access, not UI copy.
  if (ts.isElementAccessExpression(p) && p.argumentExpression === node) {
    return true;
  }
  return false;
}

function isKeyboardEventValue(node) {
  if (!KEYBOARD_EVENT_VALUES.has(node.text)) return false;
  const parent = node.parent;
  if (!parent || !ts.isBinaryExpression(parent)) return false;
  const other = parent.left === node ? parent.right : parent.right === node ? parent.left : null;
  return (
    !!other &&
    ts.isPropertyAccessExpression(other) &&
    (other.name.text === "key" || other.name.text === "code")
  );
}

// Collect translatable units from a single source file.
// Returns: [{ start, end, text, kind }]
//   kind: "jsx-text" | "string" | "template"
//   text: the exact lookup key (normalized for jsx-text)
//   start/end: byte span in `source` to replace
function collectUnits(source, fileName) {
  const scriptKind = fileName.endsWith(".tsx")
    ? ts.ScriptKind.TSX
    : fileName.endsWith(".ts")
      ? ts.ScriptKind.TS
      : ts.ScriptKind.TSX;
  const sf = ts.createSourceFile(
    fileName,
    source,
    ts.ScriptTarget.Latest,
    /* setParentNodes */ true,
    scriptKind,
  );

  const units = [];

  function visit(node) {
    if (node.kind === ts.SyntaxKind.JsxText) {
      const raw = source.substring(node.pos, node.end);
      const firstNW = raw.search(/\S/);
      if (firstNW !== -1) {
        const lastNW = raw.length - 1 - [...raw].reverse().join("").search(/\S/);
        const normalized = raw.slice(firstNW, lastNW + 1).replace(/\s+/g, " ");
        // Skip pure punctuation / entity-only / brace fragments.
        if (/[A-Za-z]/.test(normalized)) {
          units.push({
            start: node.pos + firstNW,
            end: node.pos + lastNW + 1,
            text: normalized,
            kind: "jsx-text",
          });
        }
      }
    } else if (ts.isStringLiteral(node)) {
      if (
        !isModuleSpecifier(node) &&
        !isTypePosition(node) &&
        !isDeclarationName(node) &&
        !isKeyboardEventValue(node)
      ) {
        const attr = getEnclosingJsxAttrName(node);
        if (!attr || !SKIP_JSX_ATTRS.has(attr)) {
          units.push({
            start: node.getStart(sf),
            end: node.getEnd(),
            text: node.text,
            kind: "string",
          });
        }
      }
    } else if (ts.isNoSubstitutionTemplateLiteral(node)) {
      units.push({
        start: node.getStart(sf),
        end: node.getEnd(),
        text: node.text,
        kind: "template",
      });
    }
    ts.forEachChild(node, visit);
  }

  visit(sf);
  return units;
}

module.exports = { collectUnits, SKIP_JSX_ATTRS };
