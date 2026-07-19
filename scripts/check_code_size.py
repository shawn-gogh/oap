#!/usr/bin/env python3
"""Check simple Rust code size limits for CI."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


MAX_FILE_LINES = 300
MAX_FUNCTION_LOC = 50
FN_RE = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")
DEFAULT_BASELINE = Path(__file__).with_name("code_size_baseline.json")


@dataclass
class Function:
    name: str
    start_line: int
    end_line: int
    loc: int


@dataclass
class Baseline:
    files: dict[str, int]
    functions: dict[str, int]


def tracked_rust_files() -> list[Path]:
    output = subprocess.check_output(
        ["git", "ls-files", "*.rs"],
        text=True,
    )
    return [Path(line) for line in output.splitlines()]


def strip_comments_and_literals(source: str) -> list[str]:
    lines: list[str] = []
    current: list[str] = []
    i = 0
    block_depth = 0
    in_string = False
    in_char = False
    raw_hashes: int | None = None

    while i < len(source):
        char = source[i]
        next_char = source[i + 1] if i + 1 < len(source) else ""

        if char == "\n":
            lines.append("".join(current))
            current = []
            i += 1
            continue

        if block_depth:
            if char == "/" and next_char == "*":
                block_depth += 1
                i += 2
                continue
            if char == "*" and next_char == "/":
                block_depth -= 1
                i += 2
                continue
            current.append(" ")
            i += 1
            continue

        if raw_hashes is not None:
            if char == '"' and source[i + 1 : i + 1 + raw_hashes] == "#" * raw_hashes:
                current.append(" ")
                i += 1 + raw_hashes
                raw_hashes = None
                continue
            current.append(" ")
            i += 1
            continue

        if in_string:
            if char == "\\":
                current.extend("  ")
                i += 2
                continue
            if char == '"':
                in_string = False
            current.append(" ")
            i += 1
            continue

        if in_char:
            if char == "\\":
                current.extend("  ")
                i += 2
                continue
            if char == "'":
                in_char = False
            current.append(" ")
            i += 1
            continue

        if char == "/" and next_char == "/":
            current.append(" ")
            i += 2
            while i < len(source) and source[i] != "\n":
                current.append(" ")
                i += 1
            continue

        if char == "/" and next_char == "*":
            block_depth = 1
            i += 2
            continue

        if char == "r":
            match = re.match(r'r(#+)?"', source[i:])
            if match:
                raw_hashes = len(match.group(1) or "")
                current.extend(" " * (2 + raw_hashes))
                i += 2 + raw_hashes
                continue

        if char == '"':
            in_string = True
            current.append(" ")
            i += 1
            continue

        if char == "'" and not (next_char.isalnum() or next_char == "_"):
            in_char = True
            current.append(" ")
            i += 1
            continue

        current.append(char)
        i += 1

    lines.append("".join(current))
    return lines


def is_code_line(line: str) -> bool:
    stripped = line.strip()
    return bool(stripped and stripped not in {"{", "}", "};"})


def functions_in_file(path: Path) -> list[Function]:
    stripped_lines = strip_comments_and_literals(path.read_text())
    functions: list[Function] = []
    pending: tuple[str, int] | None = None
    active: tuple[str, int, int, int] | None = None
    depth = 0

    for line_number, stripped in enumerate(stripped_lines, start=1):
        fn_index = -1
        if active is None and pending is None:
            match = FN_RE.search(stripped)
            if match:
                pending = (match.group(1), line_number)
                fn_index = match.start()

        if pending is not None and active is None:
            after_fn = stripped[fn_index:] if fn_index >= 0 else stripped
            semicolon = after_fn.find(";")
            open_brace = after_fn.find("{")
            if semicolon != -1 and (open_brace == -1 or semicolon < open_brace):
                pending = None
            elif open_brace != -1:
                name, start_line = pending
                active = (name, start_line, depth, 0)
                pending = None

        if active is not None and line_number >= active[1] and is_code_line(stripped):
            name, start_line, start_depth, loc = active
            active = (name, start_line, start_depth, loc + 1)

        depth += stripped.count("{") - stripped.count("}")

        if active is not None and depth <= active[2]:
            name, start_line, _start_depth, loc = active
            functions.append(Function(name, start_line, line_number, loc))
            active = None

    return functions


def function_entries(path: Path) -> list[tuple[str, Function]]:
    occurrences: dict[str, int] = {}
    entries = []
    for function in functions_in_file(path):
        occurrence = occurrences.get(function.name, 0) + 1
        occurrences[function.name] = occurrence
        entries.append((f"{path}::{function.name}::{occurrence}", function))
    return entries


def current_baseline() -> Baseline:
    files = {}
    functions = {}
    for path in tracked_rust_files():
        line_count = len(path.read_text().splitlines())
        if line_count > MAX_FILE_LINES:
            files[str(path)] = line_count
        for key, function in function_entries(path):
            if function.loc > MAX_FUNCTION_LOC:
                functions[key] = function.loc
    return Baseline(files=files, functions=functions)


def load_baseline(path: Path) -> Baseline:
    if not path.exists():
        return Baseline(files={}, functions={})
    value = json.loads(path.read_text())
    if value.get("version") != 1:
        raise ValueError(f"unsupported code-size baseline version in {path}")
    return Baseline(
        files={str(key): int(limit) for key, limit in value.get("files", {}).items()},
        functions={
            str(key): int(limit) for key, limit in value.get("functions", {}).items()
        },
    )


def write_baseline(path: Path, baseline: Baseline) -> None:
    value = {
        "version": 1,
        "files": dict(sorted(baseline.files.items())),
        "functions": dict(sorted(baseline.functions.items())),
    }
    path.write_text(json.dumps(value, indent=2, sort_keys=False) + "\n")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    parser.add_argument("--write-baseline", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    current = current_baseline()
    if args.write_baseline:
        write_baseline(args.baseline, current)
        print(
            f"Wrote {args.baseline} with {len(current.files)} file and "
            f"{len(current.functions)} function exceptions."
        )
        return 0

    try:
        baseline = load_baseline(args.baseline)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"Code size baseline error: {error}")
        return 1

    failures: list[str] = []

    for path in tracked_rust_files():
        line_count = len(path.read_text().splitlines())
        allowed_lines = baseline.files.get(str(path), MAX_FILE_LINES)
        if line_count > allowed_lines:
            failures.append(f"{path}: {line_count} lines exceeds allowed {allowed_lines}")

        for key, function in function_entries(path):
            allowed_loc = baseline.functions.get(key, MAX_FUNCTION_LOC)
            if function.loc > allowed_loc:
                failures.append(
                    f"{path}:{function.start_line} {function.name} has "
                    f"{function.loc} LOC exceeds allowed {allowed_loc}"
                )

    if failures:
        print("Code size limits failed with new or increased violations:")
        for failure in failures:
            print(f"  - {failure}")
        return 1

    print(
        f"Code size limits passed with {len(current.files)} file and "
        f"{len(current.functions)} function baseline exceptions."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
