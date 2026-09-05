// End-to-end test of the published page, driven in a real browser.
//
// The page is the reference consumer of this spec, so the things that break it
// are browser-only: a module that will not parse, a listener that never fires,
// a Worker that cannot start. None of that is visible to node, which is why
// this exists separately from test.mjs.
//
//   node test-page.mjs <rom> [url] [french-rom] [german-rom]
//
// Given the two translated ROMs as well, this also drives the repeatable
// language role: adding, refusing and removing.

import { chromium } from "playwright";

// Without a ROM this runs the load-and-verify half only. That half is what
// catches the failures this page has actually had -- a module that will not
// parse in a browser takes the whole page down before any ROM is involved --
// and it needs no copyrighted input, so it can run on a public CI runner.
const rom = process.argv[2] || null;
const url = process.argv[3] ?? "http://localhost:8731/index.html";
// Optional extra ROMs for the repeatable "language" role. Without them the
// language half of these tests is skipped, the same way the whole extraction
// half is skipped without a base ROM.
const frRom = process.argv[4] || process.env.FR_ROM || null;
const deRom = process.argv[5] || process.env.DE_ROM || null;

let failures = 0;
const check = (name, cond, detail = "") => {
  if (cond) console.log(`  ok   ${name}`);
  else { console.log(`  FAIL ${name} ${detail}`); failures++; }
};

const browser = await chromium.launch({ args: ["--no-sandbox"] });
const page = await browser.newPage();

// Anything the page logs as an error is a failure, even if the flow survives:
// a page that works by accident is one refactor from not working.
const problems = [];
page.on("pageerror", (e) => problems.push(`pageerror: ${e.message}`));
// A cross-origin 404 is expected and is not a page bug: the page asks the dist
// branch for a published manifest first, and that branch does not exist until
// the project has been tagged. The browser logs the failed request as a console
// error regardless, and the page then falls back to the module deployed beside
// it -- which is the behaviour test-page-stale.mjs covers. Same-origin failures
// stay fatal, because those are files this build should have produced.
const pageOrigin = new URL(url).origin;
page.on("console", (m) => {
  if (m.type() !== "error") return;
  const from = m.location()?.url ?? "";
  const expected = /Failed to load resource/.test(m.text())
    && from && !from.startsWith(pageOrigin);
  if (!expected) problems.push(`console: ${m.text()}`);
});

await page.goto(url, { waitUntil: "networkidle" });

// --- the module loads and verifies ----------------------------------------
await page.waitForSelector("#about:not([hidden])", { timeout: 15000 });
check("info box appears (module fetched, hash-matched and verified)", true);
check("no fatal error shown", await page.isHidden("#fatal"));
check("declares its input", (await page.textContent("#io-in")).includes("ROM"));
check("declares its output", (await page.textContent("#io-out")).includes(".dat"));

// --- the language switch ---------------------------------------------------
// The page ships three locales. Switching must redraw the whole interface, not
// half of it, and must survive a reload.
// Compare a string the page owns, not one that comes from the manifest: a
// proper noun like "Zelda 3 ROM" is legitimately the same in every
// language, so it proves nothing either way.
const enRun = await page.textContent("#go");
const enWhy = await page.textContent("#why-text");
await page.selectOption("#lang", "de");
const deRun = await page.textContent("#go");
check("switching to German changes the interface", deRun !== enRun, `-> ${deRun}`);
check("German reaches the buttons too", /umwandeln/i.test(deRun), `-> ${deRun}`);
check("German reaches the explanatory text too",
  (await page.textContent("#why-text")) !== enWhy);
check("html lang attribute follows the choice",
  (await page.getAttribute("html", "lang")) === "de");

await page.selectOption("#lang", "fr");
const frRun = await page.textContent("#go");
check("French reaches the buttons too", /convertir/i.test(frRun), `-> ${frRun}`);

await page.reload({ waitUntil: "networkidle" });
await page.waitForSelector("#about:not([hidden])", { timeout: 15000 });
check("the chosen language is remembered across a reload",
  (await page.inputValue("#lang")) === "fr");

await page.selectOption("#lang", "en");
check("switching back restores English", (await page.textContent("#go")) === enRun);

// The "?" must actually do something: it was a dead tooltip once.
check("the explanation starts hidden", await page.isHidden("#why-text"));
await page.click("#why");
check("clicking ? reveals the explanation", await page.isVisible("#why-text"));
await page.click("#why");
check("clicking ? again hides it", await page.isHidden("#why-text"));

if (!rom) {
  check("page logged no errors", problems.length === 0, `-> ${JSON.stringify(problems, null, 2)}`);
  await page.screenshot({ path: process.env.PAGE_SHOT ?? "page.png", fullPage: true });
  await browser.close();
  console.log(failures === 0
    ? "\nLoad tests passed (no ROM given; extraction not exercised)."
    : `\n${failures} test(s) failed.`);
  process.exit(failures === 0 ? 0 : 1);
}

// --- the input controls are ordinary controls, not landing pads ------------
// The dashed drop pads were removed deliberately; if one comes back, this
// fails rather than quietly regressing the design.
check("no dashed drop pad is rendered", (await page.locator(".drop").count()) === 0);
const roleCount = await page.locator("#roles .role").count();
check("each role gets its own help disclosure",
  (await page.locator("#roles .role-head .why").count()) === roleCount);
check("the base role offers a plain choose button",
  /choose file/i.test(await page.textContent("#role-base .choose")));
check("nothing is chosen yet",
  /no file chosen/i.test(await page.textContent("#role-base .file-name")));

// Drag and drop still works, it just shows nothing at rest. The cue is a class
// the CSS hangs a tint on, so asserting on the class is asserting on the cue.
check("no drag cue at rest", !(await page.locator("#role-base.over").count()));
const dt = await page.evaluateHandle(() => new DataTransfer());
await page.dispatchEvent("#role-base", "dragover", { dataTransfer: dt });
check("dragging a file over a role shows a cue",
  (await page.locator("#role-base.over").count()) === 1);
await page.dispatchEvent("#role-base", "dragleave", { dataTransfer: dt });
check("the cue goes away again", (await page.locator("#role-base.over").count()) === 0);

// --- each role explains itself behind a "?" --------------------------------
for (const id of ["base", "language"]) {
  const why = `#role-${id} .role-head .why`;
  const panel = `#role-${id} .role-help`;
  check(`${id} help starts hidden`, await page.isHidden(panel));
  check(`${id} help is marked collapsed`,
    (await page.getAttribute(why, "aria-expanded")) === "false");
  await page.click(why);
  check(`${id} help opens on click`, await page.isVisible(panel));
  check(`${id} help is marked expanded`,
    (await page.getAttribute(why, "aria-expanded")) === "true");
}
// Assert the fact, not the phrasing: the copy gets tightened often and a test
// that pins exact wording just breaks on every edit.
check("the base help says which release is needed",
  /US\s*\(?NTSC/i.test(await page.textContent("#role-base .role-help")));
const langHelp = await page.textContent("#role-language .role-help");
check("the language help names the languages, not the hashes",
  /french/i.test(langHelp) && /german/i.test(langHelp) && /spanish/i.test(langHelp)
    && /polish/i.test(langHelp),
  `-> ${langHelp.slice(0, 160)}`);
check("the hashes are behind a further disclosure",
  await page.isHidden("#role-language .hash-list"));
await page.click("#role-language details summary");
check("and are reachable from it",
  /^[0-9A-F]{40}$/.test((await page.textContent("#role-language .hash-list code")).trim()));
for (const id of ["base", "language"]) await page.click(`#role-${id} .role-head .why`);
check("the help closes again", await page.isHidden("#role-base .role-help"));

// --- a wrong base ROM is refused, not accepted with a warning --------------
// Zelda 3 reads fixed addresses out of the US release, so an unrecognised base
// ROM is the wrong file, not a hack to be tolerated. The manifest says so with
// acceptsModified, and this is the behaviour that has to follow from it.
await page.setInputFiles("#role-base input[type=file]",
  { name: "some-other-game.sfc", mimeType: "application/octet-stream",
    buffer: Buffer.from("not a Link to the Past dump") });
await page.waitForSelector("#role-base .status.warn", { timeout: 15000 });
const refused = await page.textContent("#role-base .status");
check("a ROM that is not the US release is refused as the base",
  /no file chosen/i.test(await page.textContent("#role-base .file-name")), `-> ${refused}`);
check("and Convert stays disabled", await page.isDisabled("#go"));
check("and the refusal names the release that is needed",
  /USA, NTSC/.test(refused), `-> ${refused}`);
check("and gives both the expected hash and the file's own",
  (refused.match(/[0-9A-F]{40}/g) ?? []).length === 2, `-> ${refused}`);
check("the refusal does not pretend the file was accepted",
  !/modified|treating it/i.test(refused), `-> ${refused}`);

// A translated ROM in the base slot is a specific mistake, so it gets a
// specific answer rather than "not one of the supported releases".
if (frRom) {
  await page.setInputFiles("#role-base input[type=file]", frRom);
  await page.waitForSelector("#role-base .status.warn", { timeout: 15000 });
  const misplaced = await page.textContent("#role-base .status");
  check("a translated ROM in the base slot is named as such",
    /additional language/i.test(misplaced), `-> ${misplaced}`);
  check("and is still not accepted", await page.isDisabled("#go"));
}

// --- picking a ROM gives feedback -----------------------------------------
await page.setInputFiles("#role-base input[type=file]", rom);
await page.waitForSelector("#role-base .status:not([hidden])", { timeout: 15000 });
const fileStatus = await page.textContent("#role-base .status");
check("selecting a ROM reports what it is", /link to the past|zelda/i.test(fileStatus), `-> ${fileStatus}`);
check("recognised ROM is not flagged as modified", !/modified/i.test(fileStatus), `-> ${fileStatus}`);
check("the chosen file is named beside the button",
  (await page.textContent("#role-base .file-name")).includes("Link to the Past"));
check("extract button is enabled", !(await page.isDisabled("#go")));

// --- the repeatable language role ------------------------------------------
const langRows = page.locator("#role-language .file-list li");
const langStatus = () => page.textContent("#role-language .status");

check("the language role has no drop pad either",
  (await page.locator("#role-language .drop").count()) === 0);
check("an add control is offered",
  /add language|ajouter une langue|sprache hinzuf/i.test(
    await page.textContent("#role-language .add-more")));
check("the add control takes several files at once",
  await page.getAttribute("#role-language input[type=file]", "multiple") !== null);
check("nothing is listed before anything is added", await langRows.count() === 0);
check("the role says what adding one buys you",
  /language/i.test(await page.textContent("#role-language .role-desc")));

if (frRom && deRom) {
  await page.setInputFiles("#role-language input[type=file]", frRom);
  await page.waitForFunction(
    () => document.querySelectorAll("#role-language .file-list li").length === 1,
    null, { timeout: 15000 });
  check("adding a translated ROM lists one language", await langRows.count() === 1);
  check("the listed language is named",
    /french/i.test(await langRows.first().textContent()),
    `-> ${await langRows.first().textContent()}`);

  await page.setInputFiles("#role-language input[type=file]", deRom);
  await page.waitForFunction(
    () => document.querySelectorAll("#role-language .file-list li").length === 2,
    null, { timeout: 15000 });
  check("adding a second one lists two", await langRows.count() === 2);
  check("the second language is named too",
    /german/i.test(await langRows.nth(1).textContent()),
    `-> ${await langRows.nth(1).textContent()}`);

  // The module refuses a language it already has (status 4), so the page has
  // to refuse it first and say why, rather than spending a run to find out.
  await page.setInputFiles("#role-language input[type=file]", frRom);
  await page.waitForSelector("#role-language .status.warn", { timeout: 15000 });
  check("adding the same language twice is refused", await langRows.count() === 2);
  check("and says why", /already/i.test(await langStatus()), `-> ${await langStatus()}`);

  // The base ROM in the language slot is a specific mistake with a specific
  // answer, so it gets one rather than "unrecognised".
  await page.setInputFiles("#role-language input[type=file]", rom);
  await page.waitForSelector("#role-language .status.warn", { timeout: 15000 });
  check("the base ROM is refused in the language slot", await langRows.count() === 2);
  check("and is named as the base ROM, not called unrecognised",
    /base rom/i.test(await langStatus()), `-> ${await langStatus()}`);

  // A file that is no supported release at all: refused, with its own hash so
  // the user can work out what they have.
  await page.setInputFiles("#role-language input[type=file]",
    { name: "not-a-rom.sfc", mimeType: "application/octet-stream",
      buffer: Buffer.from("this is not a Link to the Past ROM") });
  await page.waitForSelector("#role-language .status.warn", { timeout: 15000 });
  check("an unsupported ROM is refused as a language", await langRows.count() === 2);
  check("and its own hash is shown",
    /[0-9A-F]{40}/.test(await langStatus()), `-> ${await langStatus()}`);

  await langRows.nth(1).locator(".remove").click();
  check("removing one leaves the other", await langRows.count() === 1);
  check("the one left is the one not removed",
    /french/i.test(await langRows.first().textContent()));

  // A run with a language added has to work; its output legitimately differs
  // from the single-ROM reference, so only the output itself is checked here.
  await page.click("#go");
  await page.waitForSelector("#downloads a", { timeout: 120000 });
  check("converting with a language added produces output",
    /[1-9]/.test(await page.textContent("#downloads .meta")));

  // Back to base only, so the reference-hash check below compares like with
  // like: reference.json records a run from the US ROM alone.
  await langRows.first().locator(".remove").click();
  check("removing the last one empties the list", await langRows.count() === 0);
  check("the list is hidden when empty", await page.isHidden("#role-language .file-list"));
} else {
  console.log("  skip language role (no translated ROMs given)");
}

// --- extraction, with progress --------------------------------------------
const stages = new Set();
await page.exposeFunction("__stage", (s) => stages.add(s));
await page.evaluate(() => {
  const el = document.getElementById("run-status");
  new MutationObserver(() => window.__stage(el.textContent)).observe(
    el, { childList: true, characterData: true, subtree: true });
});

await page.click("#go");
await page.waitForSelector("#downloads a", { timeout: 120000 });

const seen = [...stages].filter((s) => /step \d+ of/i.test(s));
check("progress reported named stages", seen.length > 1, `-> saw ${seen.length}`);
check("stage names are shown", seen.some((s) => /graphics|dialogue|rom/i.test(s)),
  `-> ${JSON.stringify(seen.slice(0, 3))}`);

const meta = await page.textContent("#downloads .meta");
check("output matches the published reference hash", /hash matches/i.test(meta), `-> ${meta}`);
check("the hash itself is hidden until asked for", await page.isHidden("#downloads .hash-value"));
await page.click("#downloads .hash-toggle");
check("clicking reveals the hash",
  /^[0-9a-f]{64}$/.test((await page.textContent("#downloads .hash-value")).trim()));
check("download link is offered", (await page.getAttribute("#downloads a", "download")) === "zelda3_assets.dat");
check("no Game & Watch references on the page",
  !/game\s*&\s*watch|game and watch/i.test(await page.textContent("body")));

check("page logged no errors", problems.length === 0, `-> ${JSON.stringify(problems, null, 2)}`);

await page.screenshot({ path: process.env.PAGE_SHOT ?? "page.png", fullPage: true });
await browser.close();

console.log(failures === 0 ? "\nAll page tests passed." : `\n${failures} page test(s) failed.`);
process.exit(failures === 0 ? 0 : 1);
