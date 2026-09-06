#!/usr/bin/env python3
"""Emit the dist manifest for one release.

Describes this release per the GWRG distribution spec
(https://github.com/slash-proc/gwrg-dist-spec). One script serves every
project -- homebrew and emulator core alike -- and it contains no facts about
any of them. It is meant to be vendored unchanged; a copy that had to be
edited on the way in is a copy that drifts.

Everything that can be derived is derived, from the built bytes:

  homebrew  the "GWHB" envelope and gwhb_meta_t  -> display name, firmware ABI
  core      the "CORE" envelope and gnw_core_meta_t -> core name, ABI, systems[]

What cannot be derived lives in gwrg.json at the repo root: the converter
description for a project that ships one, and the per-system extras a core
binary does not carry (shortName, compression, BIOS, extension grouping).
Where the two overlap they are cross-checked, and a disagreement is fatal --
the binary wins arguments, gwrg.json only adds what the binary cannot say.

A project-specific invariant belongs in that project's own tests, not here:
this file is vendored by every repo, so a check that means something to only
one of them is dead weight in the other eight.

Usage:
  python3 scripts/make_manifest.py --bin "Zelda 3.bin" --artifact zelda3.ro \\
      --wasm tools/extractor/target/.../zelda3_restool.wasm \\
      --elf build/homebrew/zelda3_core.elf \\
      --tag v1.0.0 --repo slash-proc/zelda3-retro-go-sd --commit "$GITHUB_SHA" \\
      --out release/manifest.json
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MAKEFILE = ROOT / "Makefile"
DECLARED = ROOT / "gwrg.json"
EXTRACTOR = ROOT / "tools" / "extractor"

SCHEMA_VERSION = 1

TARGET = {
    "id": "gnw-retro-go",
    "platform": "game-and-watch",
    "label": "Game & Watch (Retro-Go SD)",
}

# --- the two envelopes -------------------------------------------------------
#
# Both start magic(4) + header_version(u16) + header_length(u16), then a struct.
# The layouts mirror the SDK headers exactly and the asserts below are the only
# thing standing between a silent misparse and a wrong manifest.

GWHB_MAGIC = b"GWHB"
# gwhb_meta_t, from sdk/include/Core/Inc/retro-go/gwhb.h. Exactly 96 bytes.
GWHB_FORMAT = "<7I32s4B32s"
GWHB_SIZE = struct.calcsize(GWHB_FORMAT)
assert GWHB_SIZE == 96, GWHB_SIZE

CORE_MAGIC = b"CORE"
# gnw_core_segment_t / gnw_core_system_t / gnw_core_meta_t, from
# sdk/include/Core/Inc/retro-go/gnw_core_meta.h, mirrored by sdk/tools/pack_core.py.
CORE_MAX_SEGMENTS = 4
CORE_MAX_SYSTEMS = 4
SEGMENT_FORMAT = "<III"
SEGMENT_SIZE = struct.calcsize(SEGMENT_FORMAT)
SYSTEM_FORMAT = "<32s16s32sIIIII8s8s"
SYSTEM_SIZE = struct.calcsize(SYSTEM_FORMAT)
assert SEGMENT_SIZE == 12 and SYSTEM_SIZE == 116, (SEGMENT_SIZE, SYSTEM_SIZE)
CORE_META_SIZE = (4 * 4 + CORE_MAX_SEGMENTS * SEGMENT_SIZE
                  + 4 + CORE_MAX_SYSTEMS * SYSTEM_SIZE
                  + 3 + 24 + 5)
assert CORE_META_SIZE == 564, CORE_META_SIZE

# gnw_parse_type_t. The firmware calls these rom/cdrom because it is thinking
# about media; the manifest calls them file/directory because an installer is
# thinking about whether a folder is one game or many.
BROWSE = {0: "file", 1: "directory"}

# The vocabulary boundary between the SDK and the spec. The SDK says "core"
# because that is what it builds; the spec says "emulator" because that is the
# word an end user knows. Neither side is wrong and neither is going to change,
# so the translation lives here, in one place, on purpose. Do not "fix" either
# side to match the other.
KIND = {"homebrew": "homebrew", "core": "emulator"}

# A core still wearing the template's clothes. Publishing one would put a
# phantom "Example Core" tab in front of users, reading roms/example/.
TEMPLATE_CORE_NAME = "example"
TEMPLATE_DIRNAME = "example"


def make_var(name: str) -> str:
    out = subprocess.check_output(
        ["make", "-f", str(MAKEFILE), "--no-print-directory", f"print-{name}"],
        cwd=ROOT,
        text=True,
    )
    return out.strip()


def cstr(raw: bytes) -> str:
    return raw.split(b"\0", 1)[0].decode("utf-8", "replace")


def digest(path: Path) -> tuple[int, str]:
    payload = path.read_bytes()
    return len(payload), hashlib.sha256(payload).hexdigest()


def envelope(path: Path, magic: bytes, meta_size: int) -> bytes:
    """The header_data of a packed binary, checked down to its length."""
    data = path.read_bytes()
    if len(data) < 8 + meta_size:
        raise SystemExit(f"{path}: too short to be a {magic.decode()} binary")
    if data[:4] != magic:
        raise SystemExit(
            f"{path}: expected {magic.decode()} magic, found {data[:4]!r} — "
            f"is this the packed binary?"
        )
    _header_version, header_length = struct.unpack_from("<HH", data, 4)
    if header_length < meta_size:
        raise SystemExit(
            f"{path}: header_length {header_length} is smaller than the "
            f"{meta_size}-byte metadata struct — this binary predates it"
        )
    return data[8:8 + meta_size]


def read_gwhb(path: Path) -> dict:
    fields = struct.unpack(GWHB_FORMAT, envelope(path, GWHB_MAGIC, GWHB_SIZE))
    (abi_version, abi_min_size, _flags, _code, _bss, _cover_off, _cover_size,
     display_name, _maj, _min, _pat, _r0, _reserved) = fields
    return {
        "abi": {"version": abi_version, "minSize": abi_min_size},
        "title": cstr(display_name),
        "systems": None,
    }


def read_core(path: Path) -> dict:
    meta = envelope(path, CORE_MAGIC, CORE_META_SIZE)
    abi_version, abi_min_size, _flags, _segments_count = struct.unpack_from("<IIII", meta, 0)

    off = 16 + CORE_MAX_SEGMENTS * SEGMENT_SIZE
    (systems_count,) = struct.unpack_from("<I", meta, off)
    off += 4
    if not 1 <= systems_count <= CORE_MAX_SYSTEMS:
        raise SystemExit(f"{path}: systems_count {systems_count} out of range")

    systems = []
    for i in range(systems_count):
        (name, dirname, exts, parse_type, _plo, _pls, _hlo, _hls,
         cheat_ext, _reserved) = struct.unpack_from(SYSTEM_FORMAT, meta, off + i * SYSTEM_SIZE)
        if parse_type not in BROWSE:
            raise SystemExit(f"{path}: unknown parse_type {parse_type}")
        entry = {
            "id": cstr(dirname),
            "longName": cstr(name),
            # The struct stores a space-separated list without dots, because
            # that is what the launcher matches on. A manifest is read by
            # installers and humans, so it gets the dots.
            "extensions": [f".{e}" for e in cstr(exts).split()],
            "browse": BROWSE[parse_type],
        }
        cheat = cstr(cheat_ext)
        if cheat:
            entry["cheatExt"] = cheat
        systems.append(entry)

    off += CORE_MAX_SYSTEMS * SYSTEM_SIZE + 3
    core_name = cstr(struct.unpack_from("<24s", meta, off)[0])

    return {
        "abi": {"version": abi_version, "minSize": abi_min_size},
        "title": core_name,
        "systems": systems,
    }


def check_not_template(header: dict) -> None:
    if header["title"].strip().lower() == TEMPLATE_CORE_NAME:
        raise SystemExit(
            "refusing to publish: this core still declares the template's "
            f"core name ({header['title']!r}). Set --core-name in the Makefile."
        )
    for s in header["systems"] or []:
        if s["id"].strip().lower() == TEMPLATE_DIRNAME:
            raise SystemExit(
                "refusing to publish: this core still declares the template's "
                f"system dirname ({s['id']!r}). Set --system dirname= in the Makefile."
            )


def load_declared() -> dict:
    """gwrg.json: the hand-written half, and only that."""
    if not DECLARED.is_file():
        return {}
    try:
        doc = json.loads(DECLARED.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise SystemExit(f"{DECLARED}: {exc}") from exc
    unknown = set(doc) - {"$comment", "tool", "systems", "uses"}
    if unknown:
        raise SystemExit(f"{DECLARED}: unknown key(s): {', '.join(sorted(unknown))}")
    return doc


# --- the converter, when a project ships one ---------------------------------


def verify_module(wasm_path: Path) -> None:
    """Refuse to publish a module that fails the gate a host applies.

    A manifest is a claim that these bytes are runnable, so the check that
    decides it runs here rather than being asserted. It is the same verify.mjs
    a browser loads, not a reimplementation of it.
    """
    proc = subprocess.run(
        ["node", "verify.mjs", str(wasm_path.resolve())],
        cwd=EXTRACTOR, text=True, capture_output=True,
    )
    if proc.returncode != 0:
        sys.stderr.write(proc.stdout)
        sys.stderr.write(proc.stderr)
        raise SystemExit(f"{wasm_path}: refusing to publish a non-conformant module")


def module_memory_ceiling() -> int:
    """The declared ceiling, read from the verifier's own policy.

    Duplicating the number would let the manifest and the gate drift, and the
    manifest is the half nobody would notice was wrong.
    """
    out = subprocess.check_output(
        ["node", "-e",
         'import("./verify.mjs").then(m => console.log(m.DEFAULT_POLICY.maxMemoryPages))'],
        cwd=EXTRACTOR, text=True,
    )
    return int(out.strip())


def build_tool(declared: dict, wasm_path: Path) -> dict:
    verify_module(wasm_path)
    size, sha256 = digest(wasm_path)

    max_output = declared.get("maxOutputBytes")
    if not isinstance(max_output, int):
        raise SystemExit("gwrg.json: tool.maxOutputBytes must be an integer")

    outputs = declared.get("outputs", [])
    for out in outputs:
        # Stated per output, never defaulted from the tool's ceiling: how big a
        # particular file may get is a judgement about that file, and a default
        # would quietly make every output as permissive as the loosest one.
        if not isinstance(out.get("maxBytes"), int):
            raise SystemExit(
                f"gwrg.json: output {out.get('id')!r} must state an integer maxBytes"
            )

    return {
        "id": declared["id"],
        "processor": {"type": "wasm", "version": 1},
        "title": declared["title"],
        # The four fields a host checks before instantiating. `url` is a plain
        # filename resolved beside this manifest, so the same manifest works
        # from the Pages mirror and from an offline bundle.
        "binary": {
            "file": wasm_path.name,
            "url": wasm_path.name,
            "bytes": size,
            "sha256": sha256,
        },
        "limits": {
            "maxMemoryPages": module_memory_ceiling(),
            "maxOutputBytes": max_output,
        },
        "options": declared.get("options", []),
        "inputs": declared["inputs"],
        "outputs": outputs,
    }


# --- systems -----------------------------------------------------------------


def merge_systems(derived: list[dict], declared: dict) -> list[dict]:
    """Derived systems plus the parts the binary cannot state.

    The binary is the authority on which systems exist, what they are called,
    which folder they use and which extensions they take. gwrg.json adds the
    short name, whether compressed ROMs work, any BIOS, and the grouping of
    extensions that travel together -- and may not contradict the binary.
    """
    out = []
    for system in derived:
        extra = declared.get(system["id"])
        if extra is None:
            raise SystemExit(
                f"gwrg.json: the binary declares system {system['id']!r} and "
                f"gwrg.json says nothing about it. Every system needs at least "
                f"shortName and compression."
            )
        unknown = set(extra) - {"shortName", "compression", "extensions", "bios"}
        if unknown:
            raise SystemExit(
                f"gwrg.json: system {system['id']!r} has unknown key(s): "
                f"{', '.join(sorted(unknown))}"
            )

        merged = dict(system)
        if "extensions" in extra:
            # Grouping is the one thing gwrg.json may add to, because the struct
            # has nowhere to put it: it knows a PC Engine CD game is a .cue, not
            # that .bin tracks come with it.
            #
            # So the rule is not equality. The *launcher-visible* extensions --
            # plain strings, and the first entry of each group -- must match the
            # binary exactly, because those are what the firmware scans for and
            # a typo there would advertise a system that ignores the file.
            # Everything after the first entry of a group is a companion, which
            # by definition the binary cannot know about.
            primary, companions = [], []
            for item in extra["extensions"]:
                if isinstance(item, list):
                    primary.append(item[0])
                    companions.extend(item[1:])
                else:
                    primary.append(item)
            if sorted(set(primary)) != sorted(set(system["extensions"])):
                raise SystemExit(
                    f"gwrg.json: system {system['id']!r} lists launcher extensions "
                    f"{sorted(set(primary))}, but the binary declares "
                    f"{sorted(set(system['extensions']))} — a group's first entry "
                    f"is the file the launcher lists; the rest travel with it"
                )
            merged["extensions"] = extra["extensions"]

        for key in ("shortName", "compression"):
            if key not in extra:
                raise SystemExit(f"gwrg.json: system {system['id']!r} is missing {key}")
            merged[key] = extra[key]
        if "bios" in extra:
            merged["bios"] = extra["bios"]

        # Field order for readability: identity, names, media, then extras.
        out.append({
            "id": merged["id"],
            "longName": merged["longName"],
            "shortName": merged["shortName"],
            "extensions": merged["extensions"],
            "browse": merged["browse"],
            "compression": merged["compression"],
            **({"cheatExt": merged["cheatExt"]} if "cheatExt" in merged else {}),
            **({"bios": merged["bios"]} if "bios" in merged else {}),
        })

    strays = set(declared) - {s["id"] for s in derived}
    if strays:
        raise SystemExit(
            f"gwrg.json: describes system(s) the binary does not declare: "
            f"{', '.join(sorted(strays))}"
        )
    return out


# --- assembly ----------------------------------------------------------------


def build_manifest(*, bin_path: Path, artifacts: list[Path], wasm_path: Path | None,
                   elf_path: Path | None, tag: str, repo: str, commit: str) -> dict:
    project_kind = make_var("PROJECT_KIND")
    if project_kind not in KIND:
        raise SystemExit(
            f"unsupported PROJECT_KIND for a dist manifest: {project_kind!r} "
            f"(expected one of {', '.join(sorted(KIND))})"
        )
    kind = KIND[project_kind]

    header = read_core(bin_path) if kind == "emulator" else read_gwhb(bin_path)
    check_not_template(header)

    declared = load_declared()
    project = make_var("CORE_NAME")
    title = header["title"] or project

    tools = []
    if "tool" in declared:
        if wasm_path is None:
            raise SystemExit("gwrg.json declares a tool; pass --wasm")
        tools.append(build_tool(declared["tool"], wasm_path))
    elif wasm_path is not None:
        raise SystemExit("--wasm given but gwrg.json declares no tool")

    target = {
        **TARGET,
        "kind": kind,
        "requiresAbi": header["abi"],
        "artifacts": [
            {"filename": p.name, "bytes": n, "sha256": h, "url": p.name}
            for p, (n, h) in ((p, digest(p)) for p in [bin_path, *artifacts])
        ],
    }

    if kind == "emulator":
        target["systems"] = merge_systems(header["systems"], declared.get("systems", {}))
    elif declared.get("systems"):
        raise SystemExit("gwrg.json: systems[] belongs to an emulator core, not a homebrew")

    if tools:
        # Every output of every tool, installed. A project whose target wants
        # only some of them says so in gwrg.json; none does today.
        target["uses"] = declared.get("uses") or [
            {"tool": t["id"], "outputs": [o["id"] for o in t["outputs"]], "required": True}
            for t in tools
        ]

    if elf_path is not None:
        size, sha256 = digest(elf_path)
        # Published and hashed like an artifact, and deliberately not one: the
        # install set is every artifact, and an ELF on the card is dead weight.
        # It rides in dist/<tag>/ so an offline bundle can still symbolicate a
        # crash from a project that has since disappeared.
        target["symbols"] = [{
            "filename": elf_path.name,
            "url": elf_path.name,
            "bytes": size,
            "sha256": sha256,
        }]

    return {
        "schemaVersion": SCHEMA_VERSION,
        "project": project,
        "title": title,
        # Where a human reads about this project. Derived from the repo rather
        # than written down, so it cannot name a repository this is not.
        "docs": f"https://github.com/{repo}#readme",
        "source": {"repo": repo, "commit": commit, "ref": tag},
        "tools": tools,
        "targets": [target],
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--bin", dest="bin_path", type=Path, required=True,
                    help="the packed binary (GWHB for a homebrew, CORE for a core)")
    ap.add_argument("--artifact", dest="artifacts", type=Path, action="append", default=[],
                    help="another file installed beside it (zelda3.ro, gba.xip); repeatable")
    ap.add_argument("--wasm", dest="wasm_path", type=Path,
                    help="the converter module, when gwrg.json declares a tool")
    ap.add_argument("--elf", dest="elf_path", type=Path,
                    help="the linked ELF, published for crash symbolication")
    ap.add_argument("--tag", required=True)
    ap.add_argument("--repo", required=True, help="owner/name")
    ap.add_argument("--commit", required=True)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()

    for label, path in [("packed binary", args.bin_path), ("extractor module", args.wasm_path),
                        ("ELF", args.elf_path), *[("artifact", a) for a in args.artifacts]]:
        if path is not None and not path.is_file():
            raise SystemExit(f"{label} not found: {path}")

    manifest = build_manifest(
        bin_path=args.bin_path, artifacts=args.artifacts, wasm_path=args.wasm_path,
        elf_path=args.elf_path, tag=args.tag, repo=args.repo, commit=args.commit,
    )
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

    target = manifest["targets"][0]
    print(f"make_manifest: wrote {args.out}")
    print(f"  project={manifest['project']!r} title={manifest['title']!r} "
          f"kind={target['kind']} ref={args.tag}")
    print(f"  requiresAbi version={target['requiresAbi']['version']} "
          f"minSize={target['requiresAbi']['minSize']}")
    for a in target["artifacts"]:
        print(f"  artifact {a['filename']!r} {a['bytes']}B sha256={a['sha256'][:16]}…")
    for s in target.get("systems", []):
        print(f"  system {s['id']}: {s['longName']!r} {s['extensions']} "
              f"browse={s['browse']} bios={len(s.get('bios', []))}")
    for t in manifest["tools"]:
        print(f"  tool {t['id']} {t['binary']['file']} sha256={t['binary']['sha256'][:16]}…")
        print(f"  produces {', '.join(o['filename'] for o in t['outputs'])}")
    for s in target.get("symbols", []):
        print(f"  symbols {s['filename']} {s['bytes']}B")


if __name__ == "__main__":
    main()
