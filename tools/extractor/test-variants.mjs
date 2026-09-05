// The ROMs the manifest offers are the ROMs the converter accepts.
//
// gwrg.json lists a hash per variant so a UI can recognise a file before
// spending a run. src/rom.rs lists the hashes the module will actually accept.
// They are the same facts written twice, and only one of them is enforced at
// runtime -- so the copy that can rot silently is the manifest's.
//
// This used to be impossible to get wrong: the old generator parsed KNOWN_ROMS
// out of rom.rs and derived the variants from it. Moving that data into
// gwrg.json made it hand-written, and hand-written copies drift. The check
// lives here rather than in scripts/make_manifest.py because that script is
// vendored unchanged by every project, and this invariant means something to
// exactly one of them.
//
//   node test-variants.mjs

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..", "..");

let failures = 0;
const check = (name, cond, detail = "") => {
  if (cond) console.log(`  ok   ${name}`);
  else { console.log(`  FAIL ${name}${detail ? ` -- ${detail}` : ""}`); failures++; }
};

// --- what the module accepts ------------------------------------------------

const romSrc = readFileSync(join(here, "src", "rom.rs"), "utf8");

// Parse the table itself rather than scraping every 40-hex string in the file:
// a hash in a comment, or in a test fixture, is not a ROM this module accepts.
const block = romSrc.match(
  /pub const KNOWN_ROMS: &\[\(&str, &str, &str\)\] = &\[(.*?)\n\];/s,
);
check("rom.rs declares a KNOWN_ROMS table", Boolean(block));
if (!block) process.exit(1);

const usMatch = romSrc.match(/pub const ZELDA3_SHA1_US: &str = "([0-9a-fA-F]{40})"/);
check("rom.rs declares ZELDA3_SHA1_US", Boolean(usMatch));
if (!usMatch) process.exit(1);

const accepted = new Map(); // sha1 -> language code
for (const [, sha, code] of block[1].matchAll(
  /\(\s*(ZELDA3_SHA1_US|"[0-9a-fA-F]{40}")\s*,\s*"([^"]+)"\s*,/g,
)) {
  const hex = sha === "ZELDA3_SHA1_US" ? usMatch[1] : sha.slice(1, -1);
  accepted.set(hex.toUpperCase(), code);
}
check("KNOWN_ROMS parsed", accepted.size > 0, `${accepted.size} entries`);

// --- what the manifest offers -----------------------------------------------

const declared = JSON.parse(readFileSync(join(repo, "gwrg.json"), "utf8"));
const variants = (declared.tool?.inputs ?? []).flatMap((input) =>
  (input.variants ?? []).map((v) => ({ input: input.id, ...v })),
);
check("gwrg.json declares variants", variants.length > 0, `${variants.length} found`);

// The direction that matters: a hash the manifest offers but the module refuses
// is a manifest that lies. The user picks a ROM the page says is fine, the run
// fails, and nothing points at the manifest.
for (const v of variants) {
  const sha1 = (v.sha1 ?? "").toUpperCase();
  check(
    `${v.input}/${v.id}: accepted by the module`,
    accepted.has(sha1),
    sha1 ? `${sha1} is not in KNOWN_ROMS` : "no sha1 declared",
  );
}

// The other direction is not a failure: the module may know a ROM this project
// chooses not to advertise. Worth printing, because usually it means a
// translation was added to the module and nobody updated gwrg.json.
const offered = new Set(variants.map((v) => (v.sha1 ?? "").toUpperCase()));
const unoffered = [...accepted].filter(([sha]) => !offered.has(sha));
if (unoffered.length) {
  console.log(
    `  note ${unoffered.length} ROM(s) the module accepts but gwrg.json does not offer: ` +
      unoffered.map(([, code]) => code).join(", "),
  );
}

console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
