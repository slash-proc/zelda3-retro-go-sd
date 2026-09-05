// ABI behaviour tests: the error paths, flag handling, cancellation and
// stepped/one-shot equivalence that the happy path never reaches.
//
// Needs a real ROM, so this cannot run on a public CI runner. check.sh runs it
// whenever it is given one.
//
//   node test-abi.mjs <rom.sfc> [translated.sfc ...]
//
// Any extra ROMs are translated releases; given at least one, the language
// role checks run too.

import { extract } from "./extract.mjs";
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";

const wasm = new Uint8Array(readFileSync("./target/wasm32-unknown-unknown/release/zelda3_restool.wasm"));
const rom = new Uint8Array(readFileSync(process.argv[2]));
const langs = process.argv.slice(3).map(p => new Uint8Array(readFileSync(p)));
let bad = 0;
const check = (n, c, d="") => { console.log(c ? `  ok   ${n}` : `  FAIL ${n} ${d}`); if(!c) bad++; };

// garbage input must be refused, not crash
try {
  await extract(wasm, new Uint8Array(1024));
  check("rejects a non-Zelda 3 file", false, "-> it accepted it");
} catch (e) { check("rejects a non-Zelda 3 file", true, e.message); }

// the input list: Zelda 3 takes a base ROM plus any number of translated
// ROMs, so an empty list is the only arity error. Everything else is decided
// by content, and a file that matches no known ROM is refused by role, not by
// position.
try {
  await extract(wasm, []);
  check("rejects an empty input list", false, "-> accepted");
} catch (e) { check("rejects an empty input list", /no input/i.test(e.message), `-> ${e.message}`); }

try {
  await extract(wasm, [rom, new Uint8Array(1024)]);
  check("rejects an extra file that is not a known language ROM", false, "-> accepted");
} catch (e) {
  check("rejects an extra file that is not a known language ROM", /not a language release|not the US ROM/i.test(e.message), `-> ${e.message}`);
}

// Language roles come from content, so the ways a set of files can fail to be
// a valid build are checked here rather than left to the dialogue stage: a
// language twice over, and the base ROM offered as a translation. Both are
// things the Python refuses on its command line, and both have to be refused
// before an expensive conversion rather than after it.
if (langs.length > 0) {
  try {
    await extract(wasm, [rom, langs[0], langs[0]]);
    check("rejects the same language twice", false, "-> accepted");
  } catch (e) {
    check("rejects the same language twice", /supplied twice/i.test(e.message), `-> ${e.message}`);
  }

  try {
    await extract(wasm, [rom, rom]);
    check("rejects the US ROM offered as a translation", false, "-> accepted");
  } catch (e) {
    check("rejects the US ROM offered as a translation",
      /US ROM was supplied as a translation/i.test(e.message), `-> ${e.message}`);
  }

  // The order the host registers files in must not reach the output: the
  // module sorts languages into the kLanguages declaration order.
  if (langs.length > 1) {
    const a = await extract(wasm, [rom, langs[0], langs[1]]);
    const b = await extract(wasm, [rom, langs[1], langs[0]]);
    check("language order does not depend on registration order",
      Buffer.compare(Buffer.from(a.outputs[0].data), Buffer.from(b.outputs[0].data)) === 0,
      `-> ${a.outputs[0].data.length} vs ${b.outputs[0].data.length}`);
  }
}

// a one-file list and a bare buffer are the same request
const asList = await extract(wasm, [rom]);
const asBare = await extract(wasm, rom);
check("a one-file list matches the bare-buffer shorthand",
  Buffer.compare(Buffer.from(asList.outputs[0].data), Buffer.from(asBare.outputs[0].data)) === 0);

// reserved flag bits must be refused rather than ignored
try {
  await extract(wasm, rom, { flags: 1 << 5 });
  check("rejects reserved flag bits", false, "-> accepted");
} catch (e) { check("rejects reserved flag bits", /flag/i.test(e.message), `-> ${e.message}`); }

// a modified ROM: hash check off, as the page does for a hack
const hacked = rom.slice(); hacked[0x7FD0] ^= 0xff;
try {
  const r = await extract(wasm, hacked, { flags: 1 });
  check("runs a modified ROM with noHashCheck", r.outputs.length === 1);
} catch (e) { check("runs a modified ROM with noHashCheck", false, `-> ${e.message}`); }
try {
  await extract(wasm, hacked, { flags: 0 });
  check("refuses a modified ROM without the flag", false, "-> accepted");
} catch (e) { check("refuses a modified ROM without the flag", true); }

// noIncludeRom is accepted and changes nothing: zelda3_assets.dat never
// embeds the source ROM in the first place (the Python has no such option and
// nothing in the 165 assets is a copy of the cartridge), so there is nothing
// for the flag to leave out. It stays accepted rather than rejected so a host
// that sets it across every extractor it drives does not have to special-case
// this one.
const withRom = await extract(wasm, rom, { flags: 0 });
const without = await extract(wasm, rom, { flags: 2 });
check("noIncludeRom is a no-op, the ROM is never embedded",
  Buffer.compare(Buffer.from(withRom.outputs[0].data), Buffer.from(without.outputs[0].data)) === 0,
  `-> ${withRom.outputs[0].data.length} vs ${without.outputs[0].data.length}`);

// manifest-declared output names are enforced
try {
  await extract(wasm, rom, { expectedOutputs: ["something_else.dat"] });
  check("enforces the manifest's output list", false, "-> accepted");
} catch (e) { check("enforces the manifest's output list", /manifest declares/.test(e.message)); }

// the stepped and one-shot routes must agree
const stages = [];
const stepped = await extract(wasm, rom, { onProgress: p => stages.push(p.name) });
const sha = b => createHash("sha256").update(b).digest("hex");
check("progress reported every stage", stages.length >= 3, `-> ${stages.length}`);
// Compare against reference.json rather than a literal, so the expected hash
// is the one check.sh recorded from a run that matched the Python.
if (existsSync("reference.json")) {
  const ref = JSON.parse(readFileSync("reference.json", "utf8"));
  check("stepped output equals the recorded reference",
    sha(stepped.outputs[0].data) === ref.outputs[0].sha256);
} else {
  console.log("  skip reference comparison (no reference.json; run check.sh with a ROM)");
}

// cancellation: stop asking for steps
try {
  let n = 0;
  await extract(wasm, rom, { shouldCancel: () => ++n > 3 });
  check("cancellation stops the run", false, "-> ran to completion");
} catch (e) { check("cancellation stops the run", /cancel/i.test(e.message), `-> ${e.message}`); }

console.log(bad === 0 ? "\nAll edge-case tests passed." : `\n${bad} failed.`);
process.exit(bad ? 1 : 0);
