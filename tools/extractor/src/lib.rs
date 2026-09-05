//! Zelda 3 asset conversion as a zero-import wasm module.
//!
//! ABI version 1 (all exports; the module imports nothing at all):
//!
//! ```text
//!   memory                              the module's linear memory
//!   abi_version() -> u32                the ABI this module implements
//!   alloc(len: u32) -> u32              reserve len bytes, returns offset
//!   input_clear()                       discard any registered input files
//!   input_add(ptr: u32, len: u32) -> u32   register one input, returns index
//!   run(flags: u32) -> u32              0 = ok, else an error code
//!
//!   run_begin(flags: u32) -> u32        start a stepped run
//!   run_step() -> u32                   0 = done, 1 = more work, else error
//!   stage_count() -> u32                total number of stages
//!   stage_index() -> u32                stages completed so far
//!   stage_name_ptr(i: u32) -> u32       UTF-8 name of stage i
//!   stage_name_len(i: u32) -> u32
//!   output_count() -> u32               number of files produced
//!   output_name_ptr(i: u32) -> u32      UTF-8 file name of output i
//!   output_name_len(i: u32) -> u32
//!   output_ptr(i: u32) -> u32           bytes of output i
//!   output_len(i: u32) -> u32
//!   error_ptr() -> u32                  error message, empty when run == 0
//!   error_len() -> u32
//!   warnings_ptr() -> u32               newline-separated diagnostics, may be empty
//!   warnings_len() -> u32
//! ```
//!
//! `flags` bit 0 bypasses the ROM hash check, so a modified ROM can be
//! converted. Bit 1 asks for the source ROM to be left out of the output; it
//! is accepted and does nothing here, because `zelda3_assets.dat` never
//! embeds the cartridge -- the 165 assets are all decoded tables, and the
//! Python reference has no such option either. Remaining bits are reserved
//! and must be zero.
//!
//! The host writes each file into a buffer returned by `alloc`, registers it
//! with `input_add`, calls `run`, and reads the results back out of linear
//! memory. Inputs are a list because some projects need more than one file --
//! Zelda 3 needs a base ROM plus an optional per-language ROM. A module
//! identifies each input by its content, never by the order the host supplied
//! it in or by a host-supplied name, so a mislabelled file cannot be smuggled
//! into the wrong role. Zelda 3 takes a US base ROM plus any number of
//! translated ROMs. There is no filesystem, no
//! clock, no randomness and no host call of any kind: the module cannot
//! observe or affect anything outside the memory the host hands it.
//!
//! Progress works by returning control rather than by calling out. A module
//! that imports nothing cannot invoke a host callback, and its memory is
//! non-shared, so the host cannot watch a counter while `run` is on the stack.
//! Instead the work is divided into named stages: the host calls `run_begin`
//! and then `run_step` in a loop, and between steps it is free to report
//! progress, update a UI, or simply stop calling -- which is what cancellation
//! is. `run` remains available and is exactly that loop, for hosts that do not
//! care. See docs/spec/abi.md.

pub mod assets;
pub mod codec;
pub mod dialogue;
pub mod dungeon;
pub mod extract;
pub mod graphics;
pub mod hash;
pub mod music;
pub mod overworld;
pub mod pack;
pub mod rom;

pub const ABI_VERSION: u32 = 1;

/// Bit 0 is retired. It meant "skip the ROM hash check", which is now the
/// host's decision and not expressible here. Left unused rather than reassigned
/// so an old caller setting it gets ERR_BAD_FLAGS instead of silently getting
/// whatever bit 0 came to mean next.
pub const FLAG_NO_INCLUDE_ROM: u32 = 1 << 1;

/// The name of the single file this extractor produces.
pub const OUTPUT_NAME: &str = "zelda3_assets.dat";

/// `run` status codes. 0 is success; everything else leaves a message in
/// `error_ptr`/`error_len`.
pub const ERR_EXTRACTION: u32 = 1;
pub const ERR_BAD_FLAGS: u32 = 2;
pub const ERR_NO_SESSION: u32 = 3;
pub const ERR_INPUTS: u32 = 4;

/// `run_step` returns this while there is still work left.
pub const STEP_MORE: u32 = 1;

/// ROMs in, `zelda3_assets.dat` out. The only entry point; everything else in
/// this crate is reachable from here. This is exactly what the stepped ABI
/// does, so a native caller and a wasm host run the same code.
pub fn run_extraction(inputs: Vec<Vec<u8>>, flags: u32) -> Result<extract::Extraction, String> {
    if flags & !FLAG_NO_INCLUDE_ROM != 0 {
        return Err("unrecognised flag bits set".into());
    }
    let mut ctx =
        extract::Ctx::new(inputs).map_err(|e| e.0)?;
    for (_, phase) in extract::PHASES {
        phase(&mut ctx)?;
    }
    Ok(ctx.finish())
}

// ---------------------------------------------------------------------------
// wasm ABI
// ---------------------------------------------------------------------------

/// One produced file. The name is fixed by the module, never by the caller, so
/// a host can check it against the manifest's declared output list.
struct Output {
    name: &'static str,
    data: Vec<u8>,
}

/// Files the host has registered for the next run. Cleared by `input_clear`
/// and by every `run_begin`, so a second run cannot inherit the first's files.
static mut INPUTS: Vec<Vec<u8>> = Vec::new();

static mut OUTPUTS: Vec<Output> = Vec::new();
static mut ERROR: Vec<u8> = Vec::new();
static mut WARNINGS: Vec<u8> = Vec::new();

const EMPTY: &[u8] = &[];

/// Indexed accessors return an empty slice rather than trapping when the index
/// is out of range: a buggy host gets nothing, not a panic that discards the
/// results of a run that actually succeeded.
fn output(i: u32) -> Option<&'static Output> {
    unsafe { (&*core::ptr::addr_of!(OUTPUTS)).get(i as usize) }
}

#[no_mangle]
pub extern "C" fn abi_version() -> u32 {
    ABI_VERSION
}

#[no_mangle]
pub extern "C" fn alloc(len: u32) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len as usize);
    let ptr = buf.as_mut_ptr();
    core::mem::forget(buf);
    ptr
}

/// Discards every registered input. A host that reuses a module instance for
/// a second run calls this first; `run_begin` also does it implicitly.
#[no_mangle]
pub extern "C" fn input_clear() {
    unsafe { INPUTS = Vec::new() };
}

/// Registers one input file and returns its index. Takes ownership of the
/// buffer at `ptr`, which must have come from `alloc` and must not be reused.
#[no_mangle]
pub extern "C" fn input_add(ptr: *mut u8, len: u32) -> u32 {
    let buf = unsafe { Vec::from_raw_parts(ptr, len as usize, len as usize) };
    unsafe {
        let inputs = &mut *core::ptr::addr_of_mut!(INPUTS);
        inputs.push(buf);
        (inputs.len() - 1) as u32
    }
}

/// The whole extraction in one call. Exactly `run_begin` followed by
/// `run_step` until it stops, so a host that does not want progress reporting
/// is not driving a different code path from one that does.
#[no_mangle]
pub extern "C" fn run(flags: u32) -> u32 {
    let status = run_begin(flags);
    if status != 0 {
        return status;
    }
    loop {
        match run_step() {
            STEP_MORE => continue,
            other => return other,
        }
    }
}

#[no_mangle]
pub extern "C" fn output_count() -> u32 {
    unsafe { (*core::ptr::addr_of!(OUTPUTS)).len() as u32 }
}

#[no_mangle]
pub extern "C" fn output_name_ptr(i: u32) -> *const u8 {
    output(i).map_or(EMPTY.as_ptr(), |o| o.name.as_ptr())
}

#[no_mangle]
pub extern "C" fn output_name_len(i: u32) -> u32 {
    output(i).map_or(0, |o| o.name.len() as u32)
}

#[no_mangle]
pub extern "C" fn output_ptr(i: u32) -> *const u8 {
    output(i).map_or(EMPTY.as_ptr(), |o| o.data.as_ptr())
}

#[no_mangle]
pub extern "C" fn output_len(i: u32) -> u32 {
    output(i).map_or(0, |o| o.data.len() as u32)
}

#[no_mangle]
pub extern "C" fn error_ptr() -> *const u8 {
    unsafe { (*core::ptr::addr_of!(ERROR)).as_ptr() }
}

#[no_mangle]
pub extern "C" fn error_len() -> u32 {
    unsafe { (*core::ptr::addr_of!(ERROR)).len() as u32 }
}

#[no_mangle]
pub extern "C" fn warnings_ptr() -> *const u8 {
    unsafe { (*core::ptr::addr_of!(WARNINGS)).as_ptr() }
}

#[no_mangle]
pub extern "C" fn warnings_len() -> u32 {
    unsafe { (*core::ptr::addr_of!(WARNINGS)).len() as u32 }
}

// ---------------------------------------------------------------------------
// Stepped execution
//
// The extraction is a sequence of named stages (see extract::PHASES). A host
// drives them one at a time so that it gets control back between stages, which
// is the only way to report progress or abort under a policy that forbids both
// imports and shared memory.
// ---------------------------------------------------------------------------

/// A run in progress: the accumulating context plus how far it has got.
struct Session {
    ctx: extract::Ctx,
    next: usize,
}

static mut SESSION: Option<Session> = None;

fn session() -> Option<&'static mut Session> {
    unsafe { (&mut *core::ptr::addr_of_mut!(SESSION)).as_mut() }
}

fn fail(code: u32, msg: String) -> u32 {
    unsafe {
        ERROR = msg.into_bytes();
        SESSION = None;
    }
    code
}

#[no_mangle]
pub extern "C" fn stage_count() -> u32 {
    extract::PHASES.len() as u32
}

/// How many stages have completed. Equal to `stage_count()` once the run is
/// done, so `stage_index() / stage_count()` is a progress fraction.
#[no_mangle]
pub extern "C" fn stage_index() -> u32 {
    session().map_or(0, |s| s.next as u32)
}

#[no_mangle]
pub extern "C" fn stage_name_ptr(i: u32) -> *const u8 {
    extract::PHASES
        .get(i as usize)
        .map_or(EMPTY.as_ptr(), |(name, _)| name.as_ptr())
}

#[no_mangle]
pub extern "C" fn stage_name_len(i: u32) -> u32 {
    extract::PHASES
        .get(i as usize)
        .map_or(0, |(name, _)| name.len() as u32)
}

/// Begins a stepped run over the registered inputs, consuming them.
/// Returns 0 on success, after which the host calls `run_step` until it stops
/// returning `STEP_MORE`.
#[no_mangle]
pub extern "C" fn run_begin(flags: u32) -> u32 {
    unsafe {
        OUTPUTS = Vec::new();
        ERROR = Vec::new();
        WARNINGS = Vec::new();
        SESSION = None;
    }

    if flags & !FLAG_NO_INCLUDE_ROM != 0 {
        return fail(ERR_BAD_FLAGS, "unrecognised flag bits set".into());
    }

    // Zelda 3 takes a US base ROM plus any number of translated ROMs, so the
    // list is variable-length and roles are resolved from content by
    // `Ctx::new`. Taking the list here also means a run never sees the
    // previous run's files.
    let inputs = unsafe { core::mem::take(&mut *core::ptr::addr_of_mut!(INPUTS)) };
    if inputs.is_empty() {
        return fail(ERR_INPUTS, "no input files were registered".into());
    }

    let ctx = match extract::Ctx::new(inputs) {
        Ok(c) => c,
        // A file the module could not give a role to is the host's problem,
        // not the ROM's, so it gets its own status code.
        Err(e) => return fail(ERR_INPUTS, e.0),
    };

    unsafe { SESSION = Some(Session { ctx, next: 0 }) };
    0
}

/// Runs one stage. `STEP_MORE` means call again; 0 means the run finished and
/// the outputs are ready; anything else is an error code with a message in
/// `error_ptr`. A host that stops calling simply abandons the run -- that is
/// cancellation, and it costs nothing.
#[no_mangle]
pub extern "C" fn run_step() -> u32 {
    let s = match session() {
        Some(s) => s,
        None => return fail(ERR_NO_SESSION, "run_step called without run_begin".into()),
    };

    if let Some((_, phase)) = extract::PHASES.get(s.next) {
        if let Err(e) = phase(&mut s.ctx) {
            return fail(ERR_EXTRACTION, e);
        }
        s.next += 1;
        return STEP_MORE;
    }

    // Every stage has run: serialise and publish the results.
    let session = unsafe { (*core::ptr::addr_of_mut!(SESSION)).take() };
    let done = match session {
        Some(s) => s.ctx.finish(),
        None => return fail(ERR_NO_SESSION, "session vanished mid-run".into()),
    };
    unsafe {
        OUTPUTS = vec![Output { name: OUTPUT_NAME, data: done.data }];
        WARNINGS = done.warnings.join("\n").into_bytes();
    }
    0
}
