#!/usr/bin/env python3
"""Emit the dist manifest for one release.

Describes this release per the GWRG distribution spec
(https://github.com/slash-proc/gwrg-dist-spec). This project publishes both
halves an install needs, which is what the spec is shaped around:

  artifacts  the packed GWHB binary, built here
  tools      the asset extractor, built here from tools/extractor/

The user supplies a Super Mario World ROM; the extractor turns it into
smw_assets.dat, which the target installs alongside the binary. Neither half
is useful without the other, and publishing them from one repo is what lets a
single manifest describe the whole install.

Everything that can be derived from a built artifact is derived. The firmware
ABI requirement and the display name come out of the GWHB header; the module's
size, hash and memory ceiling come out of the wasm. Nothing here is a constant
restating something the bytes already say.

Usage:
  python3 scripts/make_manifest.py \
      --bin "Super Mario World.bin" \
      --wasm tools/extractor/target/wasm32-unknown-unknown/release/smw_restool.wasm \
      --tag v1.0.0 --repo slash-proc/smw-retro-go-sd --commit "$GITHUB_SHA" \
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
# it did. The reference extraction is ~880 KiB; the ceiling is deliberately
# loose so a Lunar Magic hack with more data still fits.
MAX_OUTPUT_BYTES = 16 * 1024 * 1024

# Super Mario World (USA). A ROM that does not hash to this is still accepted,
# because a Lunar Magic hack cannot match a known hash by construction --
# see acceptsModified below.
ROM_SHA1 = "6B47BB75D16514B6A476AA0C73A683A2A4C18765"
ROM_BYTES = 512 * 1024


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


def build_tool(wasm_path: Path) -> dict:
    verify_module(wasm_path)
    payload = wasm_path.read_bytes()

    return {
        "id": "smw-assets",
        "processor": {"type": "wasm", "version": 1},
        "title": loc(
            "Super Mario World asset extraction",
            "Extraction des ressources de Super Mario World",
            "Super Mario World Ressourcen-Extraktion",
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
        # No user-settable choices. What was here was not a choice a user could
        # make usefully: one bit bypassed the ROM hash check, which is now the
        # input's `strict` flag and the host's decision, and the other omitted
        # the ROM data from the pack, which is not something this project wants
        # offered. Stated explicitly, because an absent key cannot be told from
        # a truncated file.
        "options": [],
        # The module resolves a file's role by hashing its content, never from
        # order or a name the host supplies. This entry exists so a UI can ask
        # for the right file and reject an obviously wrong one before spending
        # a run.
        "inputs": [
            {
                "id": "base",
                "required": True,
                "repeatable": False,
                "label": loc(
                    "Super Mario World ROM",
                    "ROM de Super Mario World",
                    "Super Mario World ROM",
                ),
                "extensions": [".sfc", ".smc"],
                "maxBytes": 8 * 1024 * 1024,
                "variants": [
                    {"id": "us", "sha1": ROM_SHA1, "bytes": ROM_BYTES},
                ],
                # A Lunar Magic hack cannot match a known hash by construction,
                # so refusing every unrecognised ROM would refuse the point. The
                # host accepts one and warns; see the spec's spec/05-host.md.
                "strict": False,
            }
        ],
        "outputs": [
            {"id": "assets", "filename": "smw_assets.dat", "maxBytes": MAX_OUTPUT_BYTES},
        ],
    }


def build_manifest(*, bin_path: Path, wasm_path: Path, tag: str, repo: str, commit: str) -> dict:
    meta = read_gwhb(bin_path)
    payload = bin_path.read_bytes()

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
                "artifacts": [
                    {
                        "filename": bin_path.name,
                        "bytes": len(payload),
                        "sha256": hashlib.sha256(payload).hexdigest(),
                        "url": bin_path.name,
                    }
                ],
                # The binary alone does not boot: it reads smw_assets.dat from
                # the same directory, and only the user's own ROM can produce it.
                "uses": [{"tool": "smw-assets", "outputs": ["assets"], "required": True}],
            }
        ],
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--bin", dest="bin_path", type=Path, required=True)
    ap.add_argument("--wasm", dest="wasm_path", type=Path, required=True)
    ap.add_argument("--tag", required=True)
    ap.add_argument("--repo", required=True, help="owner/name")
    ap.add_argument("--commit", required=True)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()

    if not args.bin_path.is_file():
        raise SystemExit(f"packed binary not found: {args.bin_path}")
    if not args.wasm_path.is_file():
        raise SystemExit(f"extractor module not found: {args.wasm_path}")

    manifest = build_manifest(
        bin_path=args.bin_path,
        wasm_path=args.wasm_path,
        tag=args.tag,
        repo=args.repo,
        commit=args.commit,
    )
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

    target = manifest["targets"][0]
    tool = manifest["tools"][0]
    artifact = target["artifacts"][0]
    print(f"make_manifest: wrote {args.out}")
    print(f"  project={manifest['project']!r} title={manifest['title']!r} ref={args.tag}")
    print(
        f"  requiresAbi version={target['requiresAbi']['version']} "
        f"minSize={target['requiresAbi']['minSize']}"
    )
    print(f"  artifact {artifact['filename']!r} sha256={artifact['sha256'][:16]}…")
    print(f"  tool {tool['binary']['file']} sha256={tool['binary']['sha256'][:16]}…")
    print(f"  produces {', '.join(o['filename'] for o in tool['outputs'])}")


if __name__ == "__main__":
    main()
