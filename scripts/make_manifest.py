#!/usr/bin/env python3
"""Emit the dist manifest for one release.

Describes this release per the GWRG distribution spec
(https://github.com/slash-proc/gwrg-dist-spec). This project publishes both
halves an install needs, which is what the spec is shaped around:

  artifacts  the packed GWHB binary and zelda3.ro, built here
  tools      the asset extractor, built here from tools/extractor/

The user supplies a Zelda 3 ROM; the extractor turns it into
zelda3_assets.dat, which the target installs alongside the binary. Optional
translated ROMs add languages to the same file. Neither half is useful without
the other, and publishing them from one repo is what lets a single manifest
describe the whole install.

Everything that can be derived is derived. The firmware ABI requirement and
the display name come out of the GWHB header; the module's size, hash and
memory ceiling come out of the wasm; every accepted ROM hash comes out of the
converter's own KNOWN_ROMS table. Nothing here is a constant restating
something the code already says.

Usage:
  python3 scripts/make_manifest.py \
      --bin "Zelda 3.bin" --ro zelda3.ro \
      --wasm tools/extractor/target/wasm32-unknown-unknown/release/zelda3_restool.wasm \
      --tag v1.0.0 --repo slash-proc/zelda3-retro-go-sd --commit "$GITHUB_SHA" \
      --out release/manifest.json
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MAKEFILE = ROOT / "Makefile"
EXTRACTOR = ROOT / "tools" / "extractor"

SCHEMA_VERSION = 1

GWHB_MAGIC = b"GWHB"
# gwhb_meta_t, from sdk/include/Core/Inc/retro-go/gwhb.h. Exactly 96 bytes.
META_FORMAT = "<7I32s4B32s"
META_SIZE = struct.calcsize(META_FORMAT)

TARGET = {
    "id": "gnw-retro-go",
    "platform": "game-and-watch",
    "label": "Game & Watch (Retro-Go SD)",
}

# A run cannot produce more than this, and a host rejects a module that claims
# it did. A US-only extraction is ~670 KiB and each language ROM adds to that;
# the ceiling is deliberately loose.
MAX_OUTPUT_BYTES = 64 * 1024 * 1024

# The US cartridge is 1 MiB; translated releases are the same size or smaller.
# 8 MiB is the largest file worth reading before giving up on it.
MAX_INPUT_BYTES = 8 * 1024 * 1024

ROM_SOURCE = EXTRACTOR / "src" / "rom.rs"

# Two releases carry the `redux` code, so the code cannot be the variant id and
# the shared description cannot tell them apart in a picker. These are the only
# two entries that need a human to distinguish them, keyed by hash so a table
# reordering cannot move the label onto the wrong ROM.
VARIANT_LABELS = {
    "B2A07A59E64C498BC1B2F28728F9BF4014C8D582": "English Redux (translation release)",
    "9325C22EB0A2A1F0017157C8B620BC3A605CEDE1": "English Redux (hack release)",
}


def loc(en: str, fr: str, de: str) -> dict:
    """A localised string. `en` is the base a consumer falls back to."""
    return {"en": en, "fr": fr, "de": de}


def make_var(name: str) -> str:
    out = subprocess.check_output(
        ["make", "-f", str(MAKEFILE), "--no-print-directory", f"print-{name}"],
        cwd=ROOT,
        text=True,
    )
    return out.strip()


def read_gwhb(path: Path) -> dict:
    """Parse the GWHB envelope. Raises if this is not a packed homebrew."""
    data = path.read_bytes()
    if len(data) < 8 + META_SIZE:
        raise SystemExit(f"{path}: too short to be a GWHB binary")
    if data[:4] != GWHB_MAGIC:
        raise SystemExit(f"{path}: missing GWHB magic — is this the packed binary?")

    header_version, header_length = struct.unpack_from("<HH", data, 4)
    if header_length < META_SIZE:
        raise SystemExit(
            f"{path}: header_length {header_length} is smaller than gwhb_meta_t "
            f"({META_SIZE}) — legacy binaries carry no ABI requirement"
        )

    fields = struct.unpack_from(META_FORMAT, data, 8)
    (
        required_abi_version,
        required_abi_min_size,
        _flags,
        code_size,
        bss_size,
        _cover_offset,
        cover_size,
        display_name,
        ver_major,
        ver_minor,
        ver_patch,
        _reserved0,
        _reserved,
    ) = fields

    return {
        "header_version": header_version,
        "required_abi_version": required_abi_version,
        "required_abi_min_size": required_abi_min_size,
        "code_size": code_size,
        "bss_size": bss_size,
        "cover_size": cover_size,
        "display_name": display_name.split(b"\0", 1)[0].decode("utf-8", "replace"),
        "version": f"{ver_major}.{ver_minor}.{ver_patch}",
    }


def verify_module(wasm_path: Path) -> None:
    """Refuse to publish a module that fails the gate a host applies.

    A manifest is a claim that these bytes are runnable, so the check that
    decides that runs here rather than being asserted. It is the same
    verify.mjs a browser loads, not a reimplementation of it.
    """
    proc = subprocess.run(
        ["node", "verify.mjs", str(wasm_path.resolve())],
        cwd=EXTRACTOR,
        text=True,
        capture_output=True,
    )
    if proc.returncode != 0:
        sys.stderr.write(proc.stdout)
        sys.stderr.write(proc.stderr)
        raise SystemExit(f"{wasm_path}: refusing to publish a non-conformant module")


def module_memory_ceiling() -> int:
    """The declared memory ceiling, read from the verifier's own policy.

    Duplicating the number here would let the manifest and the gate drift, and
    the manifest is the half nobody would notice was wrong.
    """
    out = subprocess.check_output(
        [
            "node",
            "-e",
            'import("./verify.mjs").then(m => console.log(m.DEFAULT_POLICY.maxMemoryPages))',
        ],
        cwd=EXTRACTOR,
        text=True,
    )
    return int(out.strip())


def known_roms() -> list[dict]:
    """Every ROM the converter recognises, read out of its own source.

    `KNOWN_ROMS` in rom.rs is what the module hashes against at run time. The
    manifest exists so a UI can recognise the same files before spending a run,
    and the two agreeing matters more than either being pretty -- so this reads
    that table rather than restating twelve hashes a second time.
    """
    text = ROM_SOURCE.read_text(encoding="utf-8")
    block = re.search(
        r"pub const KNOWN_ROMS: &\[\(&str, &str, &str\)\] = &\[(.*?)\n\];",
        text,
        re.S,
    )
    if not block:
        raise SystemExit(f"{ROM_SOURCE}: could not find the KNOWN_ROMS table")

    us_sha1 = re.search(r'pub const ZELDA3_SHA1_US: &str = "([0-9A-Fa-f]{40})"', text)
    if not us_sha1:
        raise SystemExit(f"{ROM_SOURCE}: could not find ZELDA3_SHA1_US")

    entries = []
    for sha1, code, desc in re.findall(
        r'\(\s*(ZELDA3_SHA1_US|"[0-9A-Fa-f]{40}")\s*,\s*"([^"]+)"\s*,\s*"([^"]*)"\s*\)',
        block.group(1),
    ):
        entries.append(
            {
                "sha1": us_sha1.group(1) if sha1 == "ZELDA3_SHA1_US" else sha1.strip('"'),
                "code": code,
                "desc": desc,
            }
        )

    if not entries:
        raise SystemExit(f"{ROM_SOURCE}: KNOWN_ROMS parsed as empty")
    if entries[0]["code"] != "us":
        raise SystemExit(f"{ROM_SOURCE}: expected the US ROM first, got {entries[0]['code']!r}")
    return entries


def variants(entries: list[dict]) -> list[dict]:
    """`variants[]` entries, with ids unique even where language codes repeat."""
    seen: dict[str, int] = {}
    out = []
    for e in entries:
        n = seen.get(e["code"], 0) + 1
        seen[e["code"]] = n
        # `redux` is carried by two releases. The code is what the converter
        # uses downstream; the id only has to be unique within this input.
        vid = e["code"] if n == 1 else f"{e['code']}-{n}"
        label = VARIANT_LABELS.get(e["sha1"].upper(), e["desc"])
        out.append({"id": vid, "label": {"en": label}, "sha1": e["sha1"]})
    return out


def build_tool(wasm_path: Path) -> dict:
    verify_module(wasm_path)
    payload = wasm_path.read_bytes()

    entries = known_roms()
    us, translations = entries[0], entries[1:]

    return {
        "id": "zelda3-assets",
        "processor": {"type": "wasm", "version": 1},
        "title": loc(
            "Zelda 3 asset extraction",
            "Extraction des ressources de Zelda 3",
            "Zelda 3 Ressourcen-Extraktion",
        ),
        # The four fields a host checks before instantiating. `url` is a plain
        # filename resolved beside this manifest, so the same manifest works
        # from the Pages mirror and from an offline bundle.
        "binary": {
            "file": wasm_path.name,
            "url": wasm_path.name,
            "bytes": len(payload),
            "sha256": hashlib.sha256(payload).hexdigest(),
        },
        "limits": {
            "maxMemoryPages": module_memory_ceiling(),
            "maxOutputBytes": MAX_OUTPUT_BYTES,
        },
        # No user-settable flags. Whether an unrecognised ROM may be used is
        # `strict` on the input, decided by the host before the run, and there
        # is nothing else about this conversion for a user to choose.
        "options": [],
        # The module resolves a file's role by hashing its content, never from
        # order or a name the host supplies. These entries exist so a UI can ask
        # for the right files and reject an obviously wrong one before spending
        # a run.
        "inputs": [
            {
                "id": "base",
                "required": True,
                "repeatable": False,
                "label": loc("Base ROM", "ROM de base", "Basis-ROM"),
                "description": loc(
                    "US (NTSC) cartridge dump.",
                    "Copie de la cartouche US (NTSC).",
                    "Kopie der US-Fassung (NTSC).",
                ),
                "extensions": [".sfc", ".smc"],
                "maxBytes": MAX_INPUT_BYTES,
                "variants": variants([us]),
                # Everything is extracted from this ROM by address. A file that
                # is not the US cartridge would be read as though it were, and
                # would produce garbage rather than an error.
                "strict": True,
            },
            {
                "id": "language",
                "required": False,
                "repeatable": True,
                "label": loc("Translated ROM", "ROM traduite", "Übersetztes ROM"),
                "description": loc(
                    "Add languages from translated ROMs. One file per language.",
                    "Ajoutez des langues à partir de ROMs traduites. Un fichier par langue.",
                    "Sprachen aus übersetzten ROMs hinzufügen. Eine Datei je Sprache.",
                ),
                "extensions": [".sfc", ".smc"],
                "maxBytes": MAX_INPUT_BYTES,
                "variants": variants(translations),
                # A translation is identified by its hash and nothing else. An
                # unrecognised one leaves the converter unable to say which
                # language it is reading, so there is nothing to fall back on.
                "strict": True,
            },
        ],
        "outputs": [
            {
                "id": "assets",
                "filename": "zelda3_assets.dat",
                "maxBytes": MAX_OUTPUT_BYTES,
                "label": loc("Asset pack", "Pack de ressources", "Ressourcenpaket"),
            },
        ],
    }


def artifact(path: Path) -> dict:
    payload = path.read_bytes()
    return {
        "filename": path.name,
        "bytes": len(payload),
        "sha256": hashlib.sha256(payload).hexdigest(),
        "url": path.name,
    }


def build_manifest(
    *, bin_path: Path, ro_path: Path, wasm_path: Path, tag: str, repo: str, commit: str
) -> dict:
    meta = read_gwhb(bin_path)

    project_kind = make_var("PROJECT_KIND")
    if project_kind != "homebrew":
        raise SystemExit(f"unsupported PROJECT_KIND for a dist manifest: {project_kind!r}")

    project = make_var("CORE_NAME")
    title = meta["display_name"] or project

    return {
        "schemaVersion": SCHEMA_VERSION,
        "project": project,
        "title": title,
        # Where a human reads about this project. Derived from the repo rather
        # than written down, so it cannot name a repository this is not.
        "docs": f"https://github.com/{repo}#readme",
        "source": {"repo": repo, "commit": commit, "ref": tag},
        "tools": [build_tool(wasm_path)],
        "targets": [
            {
                **TARGET,
                "kind": "homebrew",
                "requiresAbi": {
                    "version": meta["required_abi_version"],
                    "minSize": meta["required_abi_min_size"],
                },
                # Two files, both published: the packed binary the launcher
                # starts, and the read-only data it reads from the card while
                # running. The .ro is not derived from anything the user
                # supplies, so unlike the asset pack it ships as an artifact.
                "artifacts": [artifact(bin_path), artifact(ro_path)],
                # Neither of those boots on its own: the game reads
                # zelda3_assets.dat from the same directory, and only the
                # user's own ROM can produce it.
                "uses": [{"tool": "zelda3-assets", "outputs": ["assets"], "required": True}],
            }
        ],
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--bin", dest="bin_path", type=Path, required=True)
    ap.add_argument("--ro", dest="ro_path", type=Path, required=True)
    ap.add_argument("--wasm", dest="wasm_path", type=Path, required=True)
    ap.add_argument("--tag", required=True)
    ap.add_argument("--repo", required=True, help="owner/name")
    ap.add_argument("--commit", required=True)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()

    if not args.bin_path.is_file():
        raise SystemExit(f"packed binary not found: {args.bin_path}")
    if not args.ro_path.is_file():
        raise SystemExit(f"rodata sidecar not found: {args.ro_path}")
    if not args.wasm_path.is_file():
        raise SystemExit(f"extractor module not found: {args.wasm_path}")

    manifest = build_manifest(
        bin_path=args.bin_path,
        ro_path=args.ro_path,
        wasm_path=args.wasm_path,
        tag=args.tag,
        repo=args.repo,
        commit=args.commit,
    )
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

    target = manifest["targets"][0]
    tool = manifest["tools"][0]
    print(f"make_manifest: wrote {args.out}")
    print(f"  project={manifest['project']!r} title={manifest['title']!r} ref={args.tag}")
    print(
        f"  requiresAbi version={target['requiresAbi']['version']} "
        f"minSize={target['requiresAbi']['minSize']}"
    )
    for a in target["artifacts"]:
        print(f"  artifact {a['filename']!r} {a['bytes']}B sha256={a['sha256'][:16]}…")
    for inp in tool["inputs"]:
        print(
            f"  input {inp['id']}: {len(inp['variants'])} known ROM(s), "
            f"strict={inp['strict']}, repeatable={inp['repeatable']}"
        )
    print(f"  tool {tool['binary']['file']} sha256={tool['binary']['sha256'][:16]}…")
    print(f"  produces {', '.join(o['filename'] for o in tool['outputs'])}")


if __name__ == "__main__":
    main()
