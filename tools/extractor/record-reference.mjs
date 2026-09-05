#!/usr/bin/env node
// Records the hashes of a verified extraction into reference.json, which
// manifest.mjs publishes so consumers can state what a correct run produces.
//
// Run only by check.sh, immediately after a run has been confirmed
// byte-identical to the Python reference. Generating it any other way would
// mean publishing a hash for output nobody checked.

import { readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { basename } from "node:path";

const [refPath, outPath, ...inputPaths] = process.argv.slice(2);
if (inputPaths.length === 0) {
  console.error("usage: record-reference.mjs <reference.json> <output.dat> <input...>");
  process.exit(2);
}

const sha = (algo, buf) => createHash(algo).update(buf).digest("hex");
const out = readFileSync(outPath);

// The reference records every input the run was given, so a consumer can tell
// whether the files in front of it are the ones the published hash describes.
const reference = {
  inputs: inputPaths.map((p) => {
    const buf = readFileSync(p);
    return { name: basename(p), sha1: sha("sha1", buf).toUpperCase(), bytes: buf.length };
  }),
  flags: 0,
  outputs: [
    { name: "zelda3_assets.dat", bytes: out.length, sha256: sha("sha256", out) },
  ],
};

writeFileSync(refPath, JSON.stringify(reference, null, 2) + "\n");
console.log(`wrote ${refPath} (${reference.outputs[0].sha256})`);
