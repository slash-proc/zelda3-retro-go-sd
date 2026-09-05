# Zelda 3 converter: project specifics

The portable contract is in [`docs/spec/`](../../docs/spec/). This file is what
is particular to Zelda 3.

> **Status: complete.** All 165 assets are ported and `src/extract.rs` drives
> the full 29-stage pipeline. Output is byte-identical to the Python for the
> US build and for the `fr`, `de` and `de,fr` language builds.

## Input

The ABI takes a list of files. Zelda 3 declares two roles. Role is resolved
from file content, never from the order the host supplied the files in.

| Role | Required | Repeatable | |
|---|---|---|---|
| `base` | yes | no | the US cartridge ROM |
| `language` | no | yes | one translated ROM per extra language wanted |

Accepted variants of `base`:

| Variant | SHA-1 | Size |
|---|---|---|
| The Legend of Zelda: A Link to the Past (USA) | `6D4F10A8B10E10DBE624CB23CF03B88BB8252973` | 1,048,576 |

Accepted variants of `language`. These are the same hashes and language codes
the Python uses (`tables/util.py`, `ZELDA3_SHA1`); note that two different
releases both carry the `redux` code.

| Code | Release | SHA-1 |
|---|---|---|
| `de` | German | `2E62494967FB0AFDF5DA1635607F9641DF7C6559` |
| `fr` | French | `229364A1B92A05167CD38609B1AA98F7041987CC` |
| `fr-c` | French Canadian | `C1C6C7F76FFF936C534FF11F87A54162FC0AA100` |
| `en` | European English | `7C073A222569B9B8E8CA5FCB5DFEC3B5E31DA895` |
| `es` | Spanish fan translation | `461FCBD700D1332009C0E85A7A136E2A8E4B111E` |
| `pl` | Polish fan translation | `3C4D605EEFDA1D76F101965138F238476655B11D` |
| `pt` | Portuguese fan translation | `D0D09ED41F9C373FE6AFDCCAFBF0DA8C88D3D90D` |
| `redux` | English Redux script (translation release) | `B2A07A59E64C498BC1B2F28728F9BF4014C8D582` |
| `redux` | English Redux script (hack release) | `9325C22EB0A2A1F0017157C8B620BC3A605CEDE1` |
| `nl` | Dutch fan translation | `FA8ADFDBA2697C9A54D583A1284A22AC764C7637` |
| `sv` | Swedish fan translation | `43CD3438469B2C3FE879EA2F410B3EF3CB3F1CA4` |

A base ROM whose hash is not listed is accepted only with the `noHashCheck`
flag. Hosts should set it automatically for an unrecognised file rather than
exposing it as a control; it is not a decision a user can make usefully. With
that flag set, the first registered file is taken as the base.

A `language` ROM whose hash is not listed is refused, because there is nothing
to fall back on: the converter would not know which language it was reading.

Roles are resolved, and the whole set is validated, before any work starts. A
set with no base ROM, an unrecognised extra file, the same language twice, or
the US ROM offered as a translation is refused with status **4** and a message
naming the problem.

**Language order is fixed by the module, not by the host.** The Python packs
languages in the order given on `--languages`, and `de,fr` genuinely produces
a different file from `fr,de`. A host registering files has no meaningful
order to offer, so the module sorts translations into the declaration order of
`kLanguages` (`de`, `fr`, `fr-c`, `en`, `es`, `pl`, `pt`, `redux`, `nl`, `sv`).
The same set of ROMs therefore always produces the same bytes, and the oracle
to compare a build against is the Python run with the languages in that order.

## Output

| File | |
|---|---|
| `zelda3_assets.dat` | asset pack consumed by the Zelda 3 port |

`reference.json` is written **only** by `check.sh`, immediately after a run has
been confirmed byte-identical to the Python, so it cannot claim a hash nobody
checked. Without it the manifest publishes `reference: null`.

Byte-exact against the Python oracle (ORACLES.md), from the US ROM plus the
listed translations:

| Build | Bytes | sha256 |
|---|---|---|
| US only | 683,888 | `0fe2e4bd...9292c793` |
| US + `fr` | 723,536 | `6c09b76e...685a2ba` |
| US + `de` | 725,600 | `415bc151...b31849ed8` |
| US + `de,fr` | 765,256 | `b4ccc5de...0d692a90d` |

## Flags

| Bit | Name | |
|---|---|---|
| 0 | `noHashCheck` | accept a base ROM whose hash is unknown |
| 1 | `noIncludeRom` | omit the source ROM from the output |

Unrecognised bits are rejected rather than ignored. Bit 1 is declared and
accepted but has no effect here: `zelda3_assets.dat` never embeds the
cartridge, so there is nothing for it to leave out. It stays accepted so a
host that sets it across every extractor it drives need not special-case this
one.

## Status codes

| Code | |
|---|---|
| 1 | conversion failed; message explains |
| 2 | unrecognised flag bits |
| 3 | `run_step` called without `run_begin` |
| 4 | the registered input files could not be given roles |

## Provenance and coverage

`tables/restool.py`, `extract_resources.py`, `compile_resources.py`,
`util.py` and the rest of `tables/*.py` are the reference implementation. They
are **not** dead code: they are the oracle the port is checked against, and
they must keep working.

All 165 assets are ported. `./check.sh <us-rom> [translated-rom ...]` builds
both sides for every language set and diffs them; each area module also has
ignored unit tests that check its own assets against a Python-built `.dat`
(`ZELDA3_ROM`, `ZELDA3_ORACLE`, `ZELDA3_ORACLE_DAT`, `ZELDA3_ORACLE_DIR`).

**Only three of the twelve accepted ROMs have ever been run through this.**
Parity is proven for US, French and German, in every combination, because
those are the ROMs available to the author. The other nine language releases
in the manifest -- French (Canada), English (Europe), and the Spanish, Polish,
Portuguese, Dutch, Swedish and two Redux fan translations -- are ported and
their hashes are declared, but no test has ever fed one in. `pt` is the
riskiest of them: it is the one language whose font takes a different code
path (a tile remap and a third width byte), and that path has never executed.
If you have one of those ROMs, run `./check.sh` with it and say what happened.

The ROM-free tests cover the harness only: the verifier's rules, the module's
conformance, the ABI's error paths, and the page loading in a real browser. Do
not read a green public CI run as evidence that an asset is produced
correctly -- only a `check.sh` run against a real ROM shows that.

The parity check will need a copyrighted ROM, so it cannot run on a public CI
runner. It is gated behind `vars.HAVE_ZELDA3_ROM` plus a `ZELDA3_ROM_BASE64`
secret and skips when unset; maintainers run `./check.sh <rom>` locally.
Everything that does not need a ROM (build, verifier suite, conformance,
browser smoke test) runs on every push.
