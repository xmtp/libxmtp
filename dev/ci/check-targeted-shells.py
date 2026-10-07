#!/usr/bin/env python3
"""Reject CI shell calls that can enter the full local development shell."""

import argparse
import itertools
import json
from pathlib import Path
import re

import yaml

ROOT = Path(__file__).resolve().parents[2]
UNKNOWN = object()
SHELLS = {"rust", "wasm", "js", "js-node", "docs", "android", "ios"}
WRAPPER = re.compile(r"\bdev/nix-shell\b")
JUST = re.compile(r"(?:^|[;&|]\s*|\s)just\s+")
MATRIX = re.compile(r"^\$\{\{\s*matrix\.([a-zA-Z0-9_.-]+)\s*\}\}$")
TERNARY = re.compile(
    r"^\$\{\{\s*[a-zA-Z0-9_.-]+\s*&&\s*'([^']+)'\s*\|\|\s*'([^']+)'\s*\}\}$"
)


def resolve(value, matrix):
    if not isinstance(value, str):
        return set()
    if value in SHELLS:
        return {value}
    match = MATRIX.fullmatch(value)
    if match:
        selected = matrix
        for key in match[1].split("."):
            selected = selected.get(key) if isinstance(selected, dict) else None
        return {selected} if isinstance(selected, str) and selected in SHELLS else set()
    conditional = TERNARY.fullmatch(value)
    if conditional and all(choice in SHELLS for choice in conditional.groups()):
        return set(conditional.groups())
    return set()


def alternatives(value):
    if not isinstance(value, str):
        return [value]
    match = re.fullmatch(r"\$\{\{\s*fromJSON\((.*)\)\s*\}\}", value)
    if not match:
        return [None]
    body = match[1].strip()
    direct = re.fullmatch(r"'([^']*)'", body)
    branches = re.fullmatch(r".+?\s*&&\s*'([^']*)'\s*\|\|\s*'([^']*)'", body)
    raw = [direct[1]] if direct else list(branches.groups()) if branches else []
    if not raw:
        return [None]
    try:
        return [json.loads(item) for item in raw]
    except json.JSONDecodeError:
        return [None]


def expand_matrix(matrix):
    if not isinstance(matrix, dict):
        return [{}]
    axes = {
        key: alternatives(value)
        for key, value in matrix.items()
        if key not in ("include", "exclude")
    }
    scenarios = itertools.product(*axes.values()) if axes else [()]
    result = []
    for scenario in scenarios:
        values = [value if isinstance(value, list) else [UNKNOWN] for value in scenario]
        originals = (
            [dict(zip(axes, combination)) for combination in itertools.product(*values)]
            if axes
            else []
        )
        excludes = matrix.get("exclude", [])
        if not isinstance(excludes, list) or any(
            not isinstance(item, dict) for item in excludes
        ):
            raise ValueError("Matrix exclusions must be objects")
        originals = [
            row
            for row in originals
            if not any(
                all(row.get(key) == value for key, value in exclude.items())
                for exclude in excludes
            )
        ]
        for include in alternatives(matrix.get("include", [])):
            if include is None:
                result.extend(originals)
                result.append({})
                continue
            if not isinstance(include, list) or any(
                not isinstance(item, dict) for item in include
            ):
                raise ValueError("Matrix include must contain objects")
            rows = [dict(row) for row in originals]
            additions = []
            for extra in include:
                matched = False
                for original, row in zip(originals, rows):
                    if all(
                        key not in original or original[key] == value
                        for key, value in extra.items()
                    ):
                        row.update(extra)
                        matched = True
                if not matched:
                    additions.append(dict(extra))
            result.extend(rows + additions)
        if not axes and not matrix.get("include"):
            result.append({})
    return result


def matrix_rows(job):
    result = []
    for matrix in alternatives(job.get("strategy", {}).get("matrix", {})):
        result.extend(expand_matrix(matrix))
    return result


def run_shells(command, inherited, matrix, location):
    found = []
    for line in command.splitlines():
        if line.lstrip().startswith("#"):
            continue
        calls = list(WRAPPER.finditer(line))
        for call in calls:
            suffix = line[call.end() :]
            explicit = re.match(r"\s+--shell\s+['\"]?([a-zA-Z0-9_-]+)", suffix)
            prefix = line[: call.start()]
            inline = re.search(
                r"\bNIX_DEVSHELL=['\"]?([a-zA-Z0-9_-]+)['\"]?\s*$", prefix
            )
            value = explicit[1] if explicit else inline[1] if inline else inherited
            choices = resolve(value, matrix)
            if not choices:
                raise ValueError(
                    f"{location}: dev/nix-shell has no targeted shell ({value!r})"
                )
            found.extend(choices)
        if not calls and (JUST.search(line) or "${{ matrix.command }}" in line):
            choices = resolve(inherited, matrix)
            if not choices:
                raise ValueError(
                    f"{location}: Just call has no targeted shell ({inherited!r})"
                )
            found.extend(choices)
    return set(found)


def check_steps(root, steps, inherited, matrix, location, visited=()):
    found = set()
    for index, step in enumerate(steps):
        selected = step.get("env", {}).get("NIX_DEVSHELL", inherited)
        name = step.get("name", step.get("uses", f"step {index + 1}"))
        position = f"{location}/{name}"
        found.update(run_shells(step.get("run", ""), selected, matrix, position))
        action = step.get("uses", "")
        if action.startswith("./.github/actions/"):
            action_path = root / action.removeprefix("./") / "action.yml"
            if action_path in visited:
                raise ValueError(f"{position}: recursive composite action")
            contents = yaml.safe_load(action_path.read_text())
            if contents.get("runs", {}).get("using") == "composite":
                found.update(
                    check_steps(
                        root,
                        contents["runs"]["steps"],
                        selected,
                        matrix,
                        position,
                        (*visited, action_path),
                    )
                )
    return found


def audit(root):
    inventory = {}
    for path in sorted((root / ".github/workflows").glob("*.yml")):
        workflow = yaml.safe_load(path.read_text())
        inherited = workflow.get("env", {}).get("NIX_DEVSHELL")
        for name, job in workflow.get("jobs", {}).items():
            selected = job.get("env", {}).get("NIX_DEVSHELL", inherited)
            found = set()
            for row in matrix_rows(job):
                found.update(
                    check_steps(
                        root, job.get("steps", []), selected, row, f"{path.name}/{name}"
                    )
                )
            if found:
                inventory[f"{path.name}/{name}"] = sorted(found)
    helper = root / "dev/js/sdk-package"
    if "NIX_DEVSHELL=default" in helper.read_text():
        raise ValueError("dev/js/sdk-package forces the full local shell")
    return inventory


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--inventory", type=Path)
    args = parser.parse_args()
    inventory = audit(args.root.resolve())
    if args.inventory:
        args.inventory.write_text(
            json.dumps(inventory, indent=2, sort_keys=True) + "\n"
        )
    print(f"Targeted shells checked in {len(inventory)} CI jobs")


if __name__ == "__main__":
    main()
