#!/usr/bin/env node
// Diffs two zelda3_assets.dat files key by key.
//
//   node compare.mjs <oracle.dat> <ours.dat> [--all] [--quiet]
//
// The oracle is the Python's output; ours is whatever the Rust produced. The
// two are parsed independently -- no shared assumption beyond the container
// format -- and every key in the *oracle* is reported, so a partially ported
// build (fewer keys, or keys present with empty payloads) reads as a list of
// what is still missing rather than a crash.
//
// By default only differing keys are listed. --all lists every key.
// Exit status is 0 when every oracle key matches, 1 otherwise.

import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";

const MAGIC = Buffer.from("Zelda3_v0     \n\0", "latin1");
const EXPECTED_KEYS = 165;

/** Parses a .dat into { magic, keySigSha, count, keys, entries: Map<name, Buffer> }. */
function parseDat(path) {
  let buf;
  try {
    buf = readFileSync(path);
  } catch (e) {
    throw new Error(`cannot read ${path}: ${e.message}`);
  }
  if (buf.length < 88) throw new Error(`${path}: too short to be an asset pack (${buf.length} bytes)`);

  const magic = buf.subarray(0, 16);
  const keySigSha = buf.subarray(16, 48).toString("hex");
  const reserved = buf.subarray(48, 80);
  const count = buf.readUInt32LE(80);
  const keySigLen = buf.readUInt32LE(84);

  const sizesAt = 88;
  const keysAt = sizesAt + 4 * count;
  if (keysAt + keySigLen > buf.length) {
    throw new Error(
      `${path}: header claims ${count} assets and a ${keySigLen}-byte key blob, ` +
        `which runs past the end of the ${buf.length}-byte file`
    );
  }

  const sizes = [];
  for (let i = 0; i < count; i++) sizes.push(buf.readUInt32LE(sizesAt + 4 * i));

  const keyBlob = buf.subarray(keysAt, keysAt + keySigLen);
  const keys = [];
  let start = 0;
  for (let i = 0; i < keyBlob.length; i++) {
    if (keyBlob[i] === 0) {
      keys.push(keyBlob.subarray(start, i).toString("utf8"));
      start = i + 1;
    }
  }

  const problems = [];
  if (!magic.equals(MAGIC)) problems.push(`magic is ${JSON.stringify(magic.toString("latin1"))}`);
  if (!reserved.every((b) => b === 0)) problems.push("the 32 reserved bytes are not all zero");
  if (keys.length !== count) problems.push(`key blob holds ${keys.length} names but the count says ${count}`);
  const sha = createHash("sha256").update(keyBlob).digest("hex");
  if (sha !== keySigSha) problems.push(`key-blob sha256 is ${sha}, header records ${keySigSha}`);

  // Payloads: 0-3 NULs before each so it starts on a 4-byte boundary, none
  // after the last.
  const entries = new Map();
  let pos = keysAt + keySigLen;
  for (let i = 0; i < keys.length && i < sizes.length; i++) {
    while (pos & 3) pos++;
    const end = pos + sizes[i];
    if (end > buf.length) {
      problems.push(`payload for ${keys[i]} (${sizes[i]} bytes at ${pos}) runs past the end of the file`);
      entries.set(keys[i], null);
      break;
    }
    entries.set(keys[i], buf.subarray(pos, end));
    pos = end;
  }
  if (problems.length === 0 && pos !== buf.length) {
    problems.push(`${buf.length - pos} trailing byte(s) after the last payload`);
  }

  return { path, bytes: buf.length, magic, keySigSha, count, keySigLen, keys, sizes, entries, problems, sha256: createHash("sha256").update(buf).digest("hex") };
}

function main() {
  const args = process.argv.slice(2);
  const showAll = args.includes("--all");
  const quiet = args.includes("--quiet");
  const files = args.filter((a) => !a.startsWith("--"));
  if (files.length !== 2) {
    console.error("usage: node compare.mjs <oracle.dat> <ours.dat> [--all] [--quiet]");
    process.exit(2);
  }

  let oracle, ours;
  try {
    oracle = parseDat(files[0]);
    ours = parseDat(files[1]);
  } catch (e) {
    console.error(`error: ${e.message}`);
    process.exit(2);
  }

  for (const [label, d] of [["oracle", oracle], ["ours", ours]]) {
    console.log(`${label}: ${d.path}`);
    console.log(`  ${d.bytes} bytes, sha256 ${d.sha256}`);
    console.log(`  ${d.count} assets, ${d.keySigLen}-byte key blob, key-blob sha256 ${d.keySigSha}`);
    for (const p of d.problems) console.log(`  ! ${p}`);
  }
  console.log("");

  // Header equality is a separate, earlier check: it is what every porting
  // slice depends on and it can be right long before any payload is.
  const headerSame = oracle.keySigSha === ours.keySigSha && oracle.count === ours.count;
  console.log(
    headerSame
      ? `header: MATCH (${ours.count} keys, key-blob sha256 ${ours.keySigSha})`
      : `header: DIFFER (oracle ${oracle.count} keys / ${oracle.keySigSha}, ours ${ours.count} keys / ${ours.keySigSha})`
  );

  const order = oracle.keys.length ? oracle.keys : ours.keys;
  const total = order.length || EXPECTED_KEYS;

  const rows = [];
  let match = 0, missing = 0, empty = 0, differ = 0;
  for (const name of order) {
    const a = oracle.entries.get(name);
    const b = ours.entries.has(name) ? ours.entries.get(name) : undefined;
    let status, ourSize;
    if (b === undefined) {
      status = "MISSING";
      ourSize = "-";
      missing++;
    } else if (b === null) {
      status = "TRUNCATED";
      ourSize = "?";
      differ++;
    } else {
      ourSize = String(b.length);
      if (b.length === 0 && a && a.length !== 0) {
        status = "EMPTY";
        empty++;
      } else if (a && b.equals(a)) {
        status = "ok";
        match++;
      } else {
        status = "DIFFER";
        differ++;
      }
    }
    rows.push({ name, oracleSize: a ? String(a.length) : "-", ourSize, status });
  }

  // Keys we produced that the oracle does not have: an ordering bug, and worth
  // shouting about because it also breaks the key-blob hash.
  const extra = ours.keys.filter((k) => !oracle.entries.has(k));

  const shown = showAll ? rows : rows.filter((r) => r.status !== "ok");
  if (!quiet && shown.length) {
    const w = Math.max(...rows.map((r) => r.name.length), 4);
    console.log("");
    console.log(`${"key".padEnd(w)}  ${"oracle".padStart(8)}  ${"ours".padStart(8)}  status`);
    console.log("-".repeat(w + 30));
    for (const r of shown) {
      console.log(`${r.name.padEnd(w)}  ${r.oracleSize.padStart(8)}  ${r.ourSize.padStart(8)}  ${r.status}`);
    }
  }

  if (extra.length) {
    console.log("");
    console.log(`keys present in ours but not in the oracle: ${extra.join(", ")}`);
  }

  console.log("");
  const parts = [];
  if (differ) parts.push(`${differ} differ`);
  if (empty) parts.push(`${empty} empty`);
  if (missing) parts.push(`${missing} missing`);
  console.log(`${match} of ${total} keys match${parts.length ? ` (${parts.join(", ")})` : ""}`);

  const same = match === total && total === oracle.keys.length && extra.length === 0;
  if (same && oracle.sha256 === ours.sha256) console.log("files are byte-identical");
  process.exit(same ? 0 : 1);
}

main();
