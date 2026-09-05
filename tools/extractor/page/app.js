// The page's own logic. This is a reference consumer of the extractor spec: it
// uses the same verify.mjs and extract.mjs that a consuming web builder
// does, so if the ABI drifts, this page breaks in CI before anything else does.
//
// Every string that came from the module or the manifest is inserted with
// textContent, never innerHTML. Both are data we fetched, not code we trust.

import { verify } from "./verify.mjs";
import { SUPPORTED, applyStatic, localeText, onLocaleChange, setLocale, locale, t } from "./i18n.js";

const $ = (id) => document.getElementById(id);
// The manifest is the entry point and it names the module; the page does not
// hardcode the filename, because a consuming tool cannot. This is the same
// fetch sequence a third-party web tool performs against this Pages site --
// which is the distribution channel, since release assets are not
// CORS-fetchable. See docs/spec/distribution.md.
// Overridden at build time by build-page.sh. In a published build this points
// at the tag-pinned manifest on the dist branch, so the page fetches its
// extractor the same way any other consumer would.
const DEFAULT_MANIFEST = "manifest.json";
const RUN_TIMEOUT_MS = 120_000;

// `files` holds the accepted entries for each input role, keyed by role id.
// The value is always an array of { bytes, sha1, name, variant }: a role the
// manifest marks repeatable can hold several files, any other role holds at
// most one. A role the user has not filled is absent.
const state = { wasmBytes: null, tool: null, manifest: null, files: new Map(), lastResults: null };

const setStatus = (el, cls, text) => {
  el.hidden = false;
  el.className = `status ${cls}`;
  el.textContent = text;
};

const hex = (buf) =>
  [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, "0")).join("");

async function digest(algo, bytes) {
  return hex(await crypto.subtle.digest(algo, bytes));
}

const roles = () => state.tool?.inputs ?? [];
const requiredRoles = () => roles().filter((r) => r.required);
const filesFor = (roleId) => state.files.get(roleId) ?? [];
// Manifest order, flattened. The module identifies each file by content, so
// this order is a convenience for the reader, not a contract.
const allFiles = () => roles().flatMap((r) => filesFor(r.id));

// --- load and verify the module -------------------------------------------
//
// This happens silently. Verification is not a feature the user asked for and
// they cannot act on its details; it either passes, in which case saying so is
// noise, or it fails, in which case the page cannot work and must say why.

// Revalidate rather than trusting the cache. A manifest names its module and
// that module's hash, so a stale manifest fetches a stale module -- and because
// the pair is internally consistent the hash check passes, leaving the
// staleness to surface later as a confusing verification failure. This has
// happened; do not remove the cache option.
const FRESH = { cache: "no-cache" };

/**
 * Fetches a manifest and the module it describes, and checks both. Returns the
 * loaded pair, or throws with the reason this source is unusable.
 */
async function loadFrom(manifestUrl) {
  const manRes = await fetch(manifestUrl, FRESH).catch(() => null);
  if (!manRes || !manRes.ok) throw new Error(t().fatal.noManifest);
  const manifest = await manRes.json();

  const tool = manifest.tools?.[0];
  if (!tool) throw new Error("manifest declares no tools");
  if (manifest.spec !== 1) {
    throw new Error(`manifest declares spec ${manifest.spec}, this page reads spec 1`);
  }

  const moduleUrl = new URL(tool.module.url ?? tool.module.file, new URL(manifestUrl, location.href));
  // Content-address the request so a new module can never be served from a
  // cache entry belonging to an older one. The hash is checked below either
  // way; this stops the wrong bytes arriving in the first place.
  if (tool.module.sha256) moduleUrl.searchParams.set("v", tool.module.sha256.slice(0, 16));
  const wasmRes = await fetch(moduleUrl, FRESH);
  if (!wasmRes.ok) throw new Error(`could not fetch ${tool.module.file} (${wasmRes.status})`);
  const bytes = new Uint8Array(await wasmRes.arrayBuffer());

  // The manifest says which bytes it describes. If they disagree, the manifest
  // is describing something other than what we are about to run, and the honest
  // response is to refuse rather than to prefer one of them.
  const sha256 = await digest("SHA-256", bytes);
  if (sha256 !== tool.module.sha256) throw new Error(t().fatal.mismatch);

  // The real gate: decided by reading the binary, not by reading the manifest.
  const result = verify(bytes);
  if (!result.ok) throw new Error(t().fatal.unsafe(result.errors.join("; ")));

  return { manifest, tool, bytes, sha256 };
}

async function loadModule() {
  try {
    let published = DEFAULT_MANIFEST;
    try {
      const cfg = await fetch("config.json", FRESH);
      if (cfg.ok) published = (await cfg.json()).manifestUrl || published;
    } catch { /* no config: fall back to the copy beside this page */ }

    // Two sources, in order of preference. The published one on the dist branch
    // is what a third-party consumer reads, and reading it here means a release
    // reaches users without redeploying this page. The copy deployed beside the
    // page is the safety net.
    //
    // The fallback covers more than an unreachable dist branch. The page and
    // the published module are versioned independently: the page ships from
    // main, the dist branch only from a tag, so between an ABI change and the
    // next release the published module is genuinely older than this page and
    // fails its checks. That is the verifier doing its job, not a broken page,
    // and the right response is to use the module this page shipped with rather
    // than to show a dead page until someone cuts a tag.
    const sources = published === DEFAULT_MANIFEST ? [published] : [published, DEFAULT_MANIFEST];
    let loaded = null;
    const reasons = [];
    for (const url of sources) {
      try {
        loaded = await loadFrom(url);
        break;
      } catch (e) {
        reasons.push(`${url}: ${e.message ?? e}`);
      }
    }
    if (!loaded) throw new Error(reasons.join("; "));
    if (reasons.length) {
      // Not shown to the user: they cannot act on it and the page works. It
      // belongs in the console for whoever is wondering why the tag is behind.
      console.info(`using the module beside this page; ${reasons.join("; ")}`);
    }

    const { manifest, tool, bytes, sha256 } = loaded;
    state.wasmBytes = bytes;
    state.tool = tool;
    state.manifest = manifest;
    state.moduleSha256 = sha256;

    buildRoleInputs();
    // The info box doubles as the check's result: it appears only on the far
    // side of the hash match and the verifier, and it is drawn from the
    // manifest, so it says the right thing for any project using this spec.
    $("about").hidden = false;
    if (manifest.source?.repo) {
      $("repo-link").href = `https://github.com/${manifest.source.repo}`;
    }
    renderLocalised();
  } catch (e) {
    const fatal = $("fatal");
    fatal.hidden = false;
    fatal.textContent = t().fatal.cannotRun(e.message ?? e);
    document.querySelectorAll(".role").forEach((d) => d.classList.add("disabled"));
  }
}

// --- localised rendering ---------------------------------------------------
//
// Everything the language switch has to redraw lives here, so switching is one
// call and cannot leave half the page in the previous language.

function renderLocalised() {
  applyStatic();
  const tool = state.tool;
  if (!tool) return;

  const title = localeText(tool.title) || state.manifest?.title || "";
  const shortTitle = state.manifest?.shortTitle ?? state.manifest?.title ?? title;
  $("title").textContent = t().app.heading(shortTitle);
  document.title = $("title").textContent;

  const outNames = tool.outputs.map((o) => o.filename).join(", ");
  const primary = requiredRoles()[0] ?? roles()[0];
  // Name the game, not the input role: "Base ROM" is a slot in this page's own
  // vocabulary and means nothing to someone who just wants to convert a game.
  $("lede-text").textContent = t().app.lede(shortTitle, outNames);

  // A single-role project reads better with the role's own name as the section
  // heading ("Your ROM") than with a generic plural.
  $("input-heading").textContent =
    roles().length === 1 ? localeText(roles()[0].label) : t().input.heading;

  $("io-out").textContent = outNames;
  renderIoInput();

  for (const role of roles()) {
    const box = document.getElementById(`role-${role.id}`);
    if (!box) continue;
    const roleLabel = box.querySelector(".role-label");
    if (roleLabel) roleLabel.textContent = localeText(role.label);
    const opt = box.querySelector(".role-optional");
    if (opt) opt.textContent = t().input.optional;
    // The manifest owns this copy. The page used to override it with its own
    // addHint string, which meant a project could not change what its inputs
    // say without a page release.
    const desc = box.querySelector(".role-desc");
    if (desc) desc.textContent = localeText(role.description);
    renderRoleHelp(role);
    renderRole(role);
  }

  // Re-render results in the new language rather than leaving stale text.
  if (state.lastResults) showResults(state.lastResults);
}

function renderIoInput() {
  const chosen = allFiles();
  const mark = $("io-in-mark");
  if (chosen.length === 0) {
    const primary = requiredRoles()[0] ?? roles()[0];
    $("io-in").textContent = primary
      ? `${localeText(primary.label)} (${(primary.extensions ?? []).join(", ")})`
      : "";
    mark.textContent = "";
    mark.className = "mark";
    return;
  }
  // Summarise what was supplied rather than echoing file names. The names are
  // already shown against the control that took them, and three long cartridge
  // dumps wrap this box onto several lines for no benefit. Say what the module
  // is being given: the required file, then how many extras and which.
  const parts = [];
  for (const role of roles()) {
    const files = filesFor(role.id);
    if (files.length === 0) continue;
    if (role.repeatable) {
      // Name the variants, since which languages went in is the useful fact.
      const named = files.map((f) => localeText(f.variant?.label) || f.name);
      parts.push(named.join(", "));
    } else {
      parts.push(localeText(role.label));
    }
  }
  $("io-in").textContent = parts.join(" + ");
  // One mark for the whole input: everything recognised, or something not.
  const allKnown = chosen.every((c) => c.variant);
  mark.textContent = allKnown ? "✓" : "!";
  mark.className = `mark ${allKnown ? "ok" : "warn"}`;
}

// --- 1. the inputs ---------------------------------------------------------
//
// No landing pads. Each role gets an ordinary button next to the name of what
// was chosen, at the same weight as any other control on the page. Dropping a
// file on a role still works; it just does not advertise itself with a
// permanent dashed box, and shows a cue only while something is over it.

function buildRoleInputs() {
  const host = $("roles");
  host.replaceChildren();

  // With a single role the section heading already names it, so repeating the
  // label directly underneath is noise.
  const showHeads = roles().length > 1;

  for (const role of roles()) {
    const box = document.createElement("div");
    box.className = "role";
    box.id = `role-${role.id}`;

    if (showHeads) {
      const head = document.createElement("div");
      head.className = "role-head";
      const label = document.createElement("span");
      label.className = "role-label";
      head.append(label);
      if (!role.required) {
        const opt = document.createElement("span");
        opt.className = "role-optional";
        head.append(opt);
      }
      // The same disclosure as the one beside the lede, for the same reason: a
      // title attribute is invisible on a touch screen, and the detail here is
      // long enough that it does not belong on the page at rest.
      const why = document.createElement("button");
      why.type = "button";
      why.className = "why";
      why.textContent = "?";
      why.setAttribute("aria-expanded", "false");
      head.append(why);
      box.append(head);

      if (role.repeatable) {
        // The one line that stays on the page: what adding another file buys
        // the user. Everything longer lives behind the "?".
        const desc = document.createElement("p");
        desc.className = "role-desc";
        box.append(desc);
      }

      const help = document.createElement("div");
      help.className = "why-text role-help";
      help.hidden = true;
      box.append(help);
      why.addEventListener("click", () => {
        help.hidden = !help.hidden;
        why.setAttribute("aria-expanded", String(!help.hidden));
      });
    }

    const input = document.createElement("input");
    input.type = "file";
    input.className = "file-input";
    if (role.extensions?.length) input.accept = role.extensions.join(",");

    if (role.repeatable) {
      // Several files at once is the normal case here, so the picker offers
      // it rather than making the user come back for each one.
      input.multiple = true;
      const list = document.createElement("ul");
      list.className = "file-list";
      list.hidden = true;
      const add = document.createElement("button");
      add.type = "button";
      add.className = "add-more";
      box.append(list, add, input);
      add.addEventListener("click", () => input.click());
    } else {
      const row = document.createElement("div");
      row.className = "file-row";
      const choose = document.createElement("button");
      choose.type = "button";
      choose.className = "choose";
      const name = document.createElement("span");
      name.className = "file-name empty";
      row.append(choose, input, name);
      box.append(row);
      choose.addEventListener("click", () => input.click());
    }

    const status = document.createElement("div");
    status.className = "status";
    status.id = `status-${role.id}`;
    status.hidden = true;
    box.append(status);

    host.append(box);

    input.addEventListener("change", (e) => {
      const picked = [...e.target.files];
      // Clearing the control means picking the same file twice in a row still
      // fires a change event, which matters for a role you can re-fill.
      e.target.value = "";
      acceptFiles(role, picked);
    });

    for (const ev of ["dragenter", "dragover"]) {
      box.addEventListener(ev, (e) => { e.preventDefault(); box.classList.add("over"); });
    }
    box.addEventListener("dragleave", (e) => {
      // Moving between children of the box is not leaving it.
      if (!box.contains(e.relatedTarget)) box.classList.remove("over");
    });
    box.addEventListener("drop", (e) => {
      e.preventDefault();
      box.classList.remove("over");
      acceptFiles(role, [...e.dataTransfer.files]);
    });
  }
}

/**
 * The languages a repeatable role accepts, by name, in the page's language.
 * Derived from the variants' language codes rather than written out, so a
 * manifest that gains a translation gains a name here for free. A code the
 * browser does not know (this manifest carries "redux", which is a script and
 * not a language) falls back to the variant's own label.
 */
function acceptedLanguages(role) {
  let names;
  try {
    names = new Intl.DisplayNames([locale()], { type: "language" });
  } catch { names = null; }
  const out = [];
  for (const v of role.variants ?? []) {
    // Region and edition are noise in a list of languages: "fr" and "fr-c"
    // are both French, and saying so twice helps nobody.
    const base = String(v.language ?? "").split("-")[0];
    // A variant whose code is not a language at all (this manifest carries
    // "redux", which is a script for a language already in the list) has
    // nothing to add here. It is still named in the hashes below.
    let name = null;
    try {
      const got = names?.of(base);
      if (got && got !== base) name = got;
    } catch { /* not a language tag: nothing to name */ }
    if (name && !out.includes(name)) out.push(name);
  }
  return out;
}

/**
 * Fills a role's "?" panel: what the role is, and for a repeatable role which
 * releases it takes. Names first. The hashes sit behind a further disclosure,
 * because a hash is only ever useful to someone holding a file they want to
 * identify, and useless decoration to everyone else.
 */
function renderRoleHelp(role) {
  const box = document.getElementById(`role-${role.id}`);
  const help = box?.querySelector(".role-help");
  if (!help) return;
  const s = t().input;

  const why = box.querySelector(".role-head .why");
  if (why) {
    why.title = s.help ?? "";
    why.setAttribute("aria-label", `${s.help ?? ""}: ${localeText(role.label)}`);
  }

  help.replaceChildren();
  // The description is already on the row for every role, repeatable or not,
  // so restating it here would be the same fact twice.
  if (!role.repeatable) {
    const about = document.createElement("p");
    about.className = "help-line";
    about.textContent = localeText(role.description);
    help.append(about);
  }

  if (role.repeatable) {
    const langs = acceptedLanguages(role);
    if (langs.length) {
      const line = document.createElement("p");
      line.className = "help-line";
      line.textContent = `${s.accepted ?? ""} ${langs.join(", ")}.`;
      help.append(line);
    }
  }

  const variants = role.variants ?? [];
  if (variants.length === 1) {
    // One accepted release: the hash is short enough to just show. Wrapping a
    // single value in a "reveal" is ceremony, not restraint.
    const line = document.createElement("p");
    line.className = "help-line";
    const code = document.createElement("code");
    code.textContent = variants[0].sha1;
    line.append("SHA-1 ", code);
    help.append(line);
  } else if (variants.length) {
    const det = document.createElement("details");
    const sum = document.createElement("summary");
    sum.textContent = s.showHashes ?? "";
    const list = document.createElement("ul");
    list.className = "hash-list";
    for (const v of variants) {
      const li = document.createElement("li");
      const name = document.createElement("span");
      name.textContent = localeText(v.label);
      const code = document.createElement("code");
      code.textContent = v.sha1;
      li.append(name, code);
      list.append(li);
    }
    det.append(sum, list);
    help.append(det);
  }
}

/** Redraws one role's list, chosen-file name and control labels. */
function renderRole(role) {
  const box = document.getElementById(`role-${role.id}`);
  if (!box) return;
  const got = filesFor(role.id);

  if (role.repeatable) {
    const list = box.querySelector(".file-list");
    list.replaceChildren();
    for (const f of got) {
      const li = document.createElement("li");
      const name = document.createElement("span");
      name.className = "file-name";
      name.textContent = f.name;
      const variant = document.createElement("span");
      variant.className = "file-variant";
      variant.textContent = f.variant ? localeText(f.variant.label) : "";
      const remove = document.createElement("button");
      remove.type = "button";
      remove.className = "remove";
      remove.textContent = t().input.remove ?? "Remove";
      remove.addEventListener("click", () => removeFile(role, f));
      li.append(name, variant, remove);
      list.append(li);
    }
    list.hidden = got.length === 0;
    box.querySelector(".add-more").textContent =
      t().input.addLanguage ?? t().input.choose;
    return;
  }

  const name = box.querySelector(".file-name");
  name.textContent = got[0] ? got[0].name : (t().input.none ?? "");
  name.classList.toggle("empty", !got[0]);
  box.querySelector(".choose").textContent = t().input.choose;
  if (got[0]) renderFileStatus(role, got[0]);
}

function renderFileStatus(role, got) {
  const status = document.getElementById(`status-${role.id}`);
  if (got.variant) {
    setStatus(status, "ok", t().input.recognised(got.name, localeText(got.variant.label)));
  } else {
    // An unrecognised file is almost always a ROM hack, which by definition
    // cannot match a known hash. Rather than making the user find and
    // understand a checkbox, accept it and say what we assumed. If it is not
    // the right game at all, the extraction fails on its own.
    setStatus(status, "warn", t().input.unrecognised(got.name, localeText(role.label)));
  }
}

/**
 * Why a file cannot fill this role, or null if it can. These are decisions,
 * not settings: every one of them has a single right answer, so the page makes
 * it and says what it did rather than offering a control.
 *
 * Whether an unrecognised file is refused at all is the manifest's call, via
 * the role's `acceptsModified`. A project whose users routinely supply hacked
 * ROMs sets it and gets a note instead of a refusal; a project that reads
 * fixed addresses out of one specific release clears it, and a file that
 * hashes to something else is simply the wrong file.
 */
function refusalFor(role, got) {
  const s = t().input;
  const existing = filesFor(role.id);
  if (role.repeatable && existing.some((f) => f.sha1 === got.sha1)) {
    return (s.alreadyAdded ?? ((n) => `${n} has already been added.`))(got.name);
  }
  // The same file is meaningful in another role, so name that role rather than
  // calling a perfectly good ROM unrecognised.
  const other = roles().find(
    (r) => r.id !== role.id && (r.variants ?? []).some((v) => v.sha1 === got.sha1));
  if (other) {
    return (s.wrongRole ?? ((n, o, r) => `${n} is the ${o}, not the ${r}.`))(
      got.name, localeText(other.label), localeText(role.label));
  }
  if (!got.variant && role.acceptsModified === false) {
    const variants = role.variants ?? [];
    // With one acceptable file, name it and its hash outright. With a dozen,
    // the list belongs behind the "?" and the message points at it; either
    // way the user is told what their own file hashed to, which is the part
    // that tells them what they are actually holding.
    if (variants.length === 1) {
      return (s.notTheOne ?? ((n, v, e, a) => `${n} is not ${v} (${e}); it hashes to ${a}.`))(
        got.name, localeText(variants[0].label), variants[0].sha1, got.sha1);
    }
    return (s.notRecognised ?? ((n, r, a) => `${n} is not a supported ${r}; it hashes to ${a}.`))(
      got.name, localeText(role.label), got.sha1);
  }
  // The module refuses a second file for a language it already has, so refuse
  // it up front and say so rather than spending a run to find out.
  const dup = role.repeatable && got.variant?.language
    && existing.some((f) => f.variant?.language === got.variant.language);
  if (dup) {
    return (s.languageAlreadyAdded ?? ((v) => `${v} has already been added.`))(
      localeText(got.variant.label));
  }
  return null;
}

function removeFile(role, got) {
  const left = filesFor(role.id).filter((f) => f !== got);
  if (left.length) state.files.set(role.id, left);
  else state.files.delete(role.id);
  const status = document.getElementById(`status-${role.id}`);
  if (status) status.hidden = true;
  renderRole(role);
  renderIoInput();
  updateGo();
}

async function acceptFiles(role, files) {
  const status = document.getElementById(`status-${role.id}`);
  if (!files.length) return;

  for (const file of files) {
    if (role.maxBytes && file.size > role.maxBytes) {
      setStatus(status, "bad", t().input.tooLarge(file.name));
      continue;
    }

    setStatus(status, "busy", t().input.reading(file.name));
    const bytes = new Uint8Array(await file.arrayBuffer());
    const sha1 = (await digest("SHA-1", bytes)).toUpperCase();
    const variant = (role.variants ?? []).find((v) => v.sha1 === sha1) ?? null;
    const got = { bytes, sha1, name: file.name, variant };

    const refusal = refusalFor(role, got);
    if (refusal) {
      // A refused file is not held on to: nothing is stored, so the Convert
      // button stays where it was and the page cannot be talked into running
      // on a file it just said no to.
      setStatus(status, "warn", refusal);
      continue;
    }

    if (!role.repeatable) {
      // One file, replacing whatever was there.
      state.files.set(role.id, [got]);
      renderFileStatus(role, got);
      continue;
    }
    state.files.set(role.id, [...filesFor(role.id), got]);
    // The list itself now says what was added, so the status line has nothing
    // left to report.
    status.hidden = true;
  }

  renderRole(role);
  renderIoInput();
  updateGo();
}

function updateGo() {
  const ready = requiredRoles().every((r) => filesFor(r.id).length > 0);
  $("go").disabled = !ready;
}

// --- 2. run ----------------------------------------------------------------

function progressBar() {
  const wrap = document.createElement("div");
  wrap.className = "bar";
  const fill = document.createElement("div");
  wrap.append(fill);
  return { wrap, fill };
}

async function run() {
  const status = $("run-status");
  const results = $("results");
  const warnList = $("warnings");
  results.hidden = warnList.hidden = true;
  warnList.replaceChildren();
  $("downloads").replaceChildren();
  $("go").disabled = true;
  state.lastResults = null;

  const { wrap, fill } = progressBar();
  status.hidden = false;
  status.className = "status busy";
  status.replaceChildren(document.createTextNode(t().run.starting), wrap);

  // Registration order follows the manifest's role order, but the module
  // identifies each file by content, so the order is a convenience only.
  const ordered = allFiles();
  // Only a role that says it accepts modified files can ask for the hash check
  // to be skipped. Everywhere else an unrecognised file was refused at the
  // picker, so there is nothing to relax and the flag stays off.
  const anyUnrecognised = roles().some(
    (r) => r.acceptsModified !== false && filesFor(r.id).some((f) => !f.variant));

  const worker = new Worker("worker.js", { type: "module" });
  // The ABI has no cancel flag, so this is what a timeout means: stop the
  // thread the module is running on.
  const timer = setTimeout(() => {
    worker.terminate();
    setStatus(status, "bad", t().run.timedOut);
    updateGo();
  }, RUN_TIMEOUT_MS);

  worker.onmessage = async (ev) => {
    const m = ev.data;
    if (m.type === "progress") {
      const pct = Math.round((m.stage / m.stages) * 100);
      fill.style.width = `${pct}%`;
      status.firstChild.textContent = t().run.progress(pct, m.name, m.stage + 1, m.stages);
      return;
    }
    clearTimeout(timer);
    worker.terminate();
    updateGo();

    if (m.type === "error") {
      setStatus(status, "bad", m.message);
      return;
    }
    state.lastResults = m;
    await showResults(m);
  };

  worker.postMessage({
    wasmBytes: state.wasmBytes,
    inputs: ordered.map((f) => f.bytes),
    flags: anyUnrecognised ? (state.tool.flags?.noHashCheck ?? 0) : 0,
    expectedOutputs: state.tool.outputs.map((o) => o.filename),
    maxOutputBytes: state.tool.limits?.maxOutputBytes,
  });
}

async function showResults({ outputs, warnings }) {
  const status = $("run-status");
  const warnList = $("warnings");
  const results = $("results");
  setStatus(status, "ok", t().run.done(outputs.length));

  warnList.replaceChildren();
  if (warnings.length) {
    warnList.hidden = false;
    for (const w of warnings) {
      const li = document.createElement("li");
      li.textContent = w;                       // module-supplied: text, never markup
      warnList.append(li);
    }
  }

  const reference = state.tool.reference;
  const list = $("downloads");
  list.replaceChildren();
  for (const out of outputs) {
    const data = new Uint8Array(out.data);
    const sha256 = await digest("SHA-256", data);

    const li = document.createElement("li");
    const a = document.createElement("a");
    a.href = URL.createObjectURL(new Blob([data], { type: "application/octet-stream" }));
    a.download = out.name;
    a.textContent = t().results.download(out.name);
    const meta = document.createElement("div");
    meta.className = "meta";

    // If this repo published the hashes of a verified reference run, and the
    // user gave the same inputs, the output should match exactly. The verdict
    // is the part that means anything to a reader; the hash itself is 64
    // characters of noise until someone actually wants to compare it, so it
    // stays behind a click.
    const expected = matchesReferenceInput(reference)
      ? reference.outputs.find((o) => o.name === out.name)
      : null;

    const toggle = document.createElement("button");
    toggle.type = "button";
    toggle.className = "hash-toggle";
    if (!expected) {
      toggle.textContent = t().results.hash;
    } else if (expected.sha256 === sha256) {
      toggle.textContent = t().results.hashMatches;
      toggle.classList.add("ok");
    } else {
      toggle.textContent = t().results.hashDiffers;
      toggle.classList.add("bad");
    }

    const hash = document.createElement("code");
    hash.className = "hash-value";
    hash.hidden = true;
    hash.textContent = sha256;
    toggle.addEventListener("click", () => { hash.hidden = !hash.hidden; });

    meta.append(`${t().results.bytes(data.length)} · `, toggle, hash);

    li.append(a, meta);
    list.append(li);
  }
  results.hidden = false;
}

/** True when the files the user supplied are the ones the reference run used. */
function matchesReferenceInput(reference) {
  if (!reference) return false;
  // Spec 1 references record either a single `input` or a list of `inputs`.
  const want = reference.inputs ?? (reference.input ? [reference.input] : []);
  if (want.length === 0) return false;
  const got = allFiles().map((f) => f.sha1).sort();
  const expect = want.map((i) => i.sha1).sort();
  return got.length === expect.length && got.every((h, i) => h === expect[i]);
}

// --- wiring ----------------------------------------------------------------

const langSelect = $("lang");
for (const l of SUPPORTED) {
  const opt = document.createElement("option");
  opt.value = l.code;
  opt.textContent = l.label;          // languages are named in their own language
  langSelect.append(opt);
}
langSelect.value = locale();
langSelect.addEventListener("change", (e) => setLocale(e.target.value));

$("go").addEventListener("click", run);

// The "?" is a disclosure, not a tooltip: it has to work on a touch screen.
const why = $("why");
why.addEventListener("click", () => {
  const box = $("why-text");
  box.hidden = !box.hidden;
  why.setAttribute("aria-expanded", String(!box.hidden));
});

onLocaleChange(renderLocalised);
document.documentElement.lang = locale();
loadModule();
