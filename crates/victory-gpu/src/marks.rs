//! Write marks (`VICTORY_VIEW_MARKS=1`, branch fx-marks): shaders translated
//! with them set a bit per 64 KiB page of title memory they write through a
//! buffer view (`victory_shader::emit::SLOT_MARKS`). The bit of a page is its
//! title address >> 16, modulo `VIEW_WRITE_BITS`; a view's table entry is the
//! bit of its first byte and that byte's offset in its page. A write through
//! a slot without an entry sets the sentinel bit (`VIEW_WRITE_BITS`): the host
//! then compares everything, as without marks.
//!
//! The bitmap lives in CPU-visible memory (write-back, pool L0): the GPU sets
//! a page's bit once (a load first, an atomic OR only when clear) and the
//! host reads it directly after the work has run.
//!
//! Stage 2 (this file): the bitmap, the table entries and the check that
//! every page the old compare finds written is marked (`MISSED` stays 0).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use victory_shader::emit::{VIEW_MARK_NONE, VIEW_WRITE_BITS};
use windows::Win32::Graphics::Direct3D12::ID3D12Resource;

/// Whether write marks are on (the translator's switch).
pub(crate) fn on() -> bool {
    victory_shader::emit::view_marks()
}

/// The table entry of a buffer view whose first byte is at title address
/// `address`: its page bit and its phase (offset in the page).
pub(crate) fn entry(address: u64) -> [u32; 2] {
    [((address >> 16) & u64::from(VIEW_WRITE_BITS - 1)) as u32, (address & 0xFFFF) as u32]
}

/// The entry of a slot that holds no buffer view (or one we cannot place).
pub(crate) const NONE: [u32; 2] = [VIEW_MARK_NONE, 0];

/// Bytes of the bitmap: the page bits and one word for the sentinel.
const BITMAP_BYTES: u64 = (VIEW_WRITE_BITS as u64 + 32) / 8;

struct Bitmap {
    buffer: ID3D12Resource,
    gpu: u64,
    data: *mut u32,
}
// SAFETY: the mapping lives as long as the buffer, which is never released;
// it is read and cleared with volatile accesses.
unsafe impl Send for Bitmap {}
unsafe impl Sync for Bitmap {}

static BITMAP: OnceLock<Option<Bitmap>> = OnceLock::new();

/// The bitmap's GPU address, made at the first ask.
pub(crate) fn bitmap_gpu(host: &crate::host::Host) -> Option<u64> {
    BITMAP
        .get_or_init(|| {
            let (buffer, data) = host.shared_buffer(BITMAP_BYTES)?;
            // SAFETY: the whole mapping of a live buffer.
            let gpu = unsafe {
                std::ptr::write_bytes(data, 0, BITMAP_BYTES as usize);
                buffer.GetGPUVirtualAddress()
            };
            crate::host::name(&buffer, || "write marks: written pages bitmap".to_owned());
            tracing::info!(bytes = BITMAP_BYTES, "write marks: bitmap made (VICTORY_VIEW_MARKS=1)");
            Some(Bitmap { buffer, gpu, data: data.cast() })
        })
        .as_ref()
        .map(|b| b.gpu)
}

/// Whether the bit of the page at title address `address` is set.
pub(crate) fn marked(address: u64) -> bool {
    let Some(Some(b)) = BITMAP.get() else {
        return true;
    };
    let n = (address >> 16) & u64::from(VIEW_WRITE_BITS - 1);
    // SAFETY: inside the mapping (n < VIEW_WRITE_BITS).
    let word = unsafe { b.data.add((n / 32) as usize).read_volatile() };
    word >> (n % 32) & 1 != 0
}

/// Whether some write found no table entry since the bitmap was cleared.
pub(crate) fn sentinel() -> bool {
    let Some(Some(b)) = BITMAP.get() else {
        return true;
    };
    // SAFETY: the sentinel's word is the last of the mapping.
    unsafe { b.data.add((VIEW_WRITE_BITS / 32) as usize).read_volatile() != 0 }
}

/// Clears the bitmap: only when no work that may mark is on the card and
/// every flight has landed (`Host::settle`).
/// The clear epoch: bumped at every clear (per-copy reliability).
static EPOCH: AtomicU64 = AtomicU64::new(0);

/// The current clear epoch.
pub(crate) fn epoch() -> u64 {
    EPOCH.load(Ordering::Relaxed)
}

thread_local! {
    /// Whether the program bound on this thread (being recorded) marks
    /// every write it makes; false outside a bind (root views, memory by
    /// address).
    static BINDING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

thread_local! {
    /// The program bound on this thread: its hash (first 16 hex digits) and marks value.
    static WRITER: std::cell::Cell<(u64, u8)> = const { std::cell::Cell::new((0, 0)) };
}

/// Diagnostics: the program now bound on this thread (write marks' miss detail).
pub(crate) fn set_writer(hash: &str, marks: u8) {
    let h = u64::from_str_radix(hash.get(..16).unwrap_or(hash), 16).unwrap_or(0);
    WRITER.with(|w| w.set((h, marks)));
}

/// The program now bound on this thread: hash and marks value.
pub(crate) fn writer() -> (u64, u8) {
    WRITER.with(|w| w.get())
}

/// The program now bound on this thread marks every write (`marks == 1`).
pub(crate) fn set_binding(marked: bool) {
    BINDING.with(|b| b.set(marked));
}

/// Whether the program now bound on this thread marks every write.
pub(crate) fn binding_marked() -> bool {
    BINDING.with(|b| b.get())
}

pub(crate) fn clear() {
    EPOCH.fetch_add(1, Ordering::Relaxed);
    if let Some(Some(b)) = BITMAP.get() {
        // SAFETY: the whole mapping; no shader writes it now.
        unsafe { std::ptr::write_bytes(b.data.cast::<u8>(), 0, BITMAP_BYTES as usize) };
        CLEARS.fetch_add(1, Ordering::Relaxed);
        UNMARKED.store(0, Ordering::Relaxed);
    }
}

/// Times the bitmap was cleared.
static CLEARS: AtomicU64 = AtomicU64::new(0);

/// Programs that store without marking every write, bound since the last
/// clear: while any has, the marks cannot be relied on.
static UNMARKED: AtomicU64 = AtomicU64::new(0);

/// A program that stores without marking every write (marks 0 or 2) was bound.
pub(crate) fn unmarked_writer() {
    if on() {
        UNMARKED.fetch_add(1, Ordering::Relaxed);
    }
}

/// Whether the marks can be relied on now: no program that stores unmarked
/// since the last clear (a write through a null slot marks nothing and
/// needs nothing).
pub(crate) fn reliable() -> bool {
    UNMARKED.load(Ordering::Relaxed) == 0
}

/// Stage 2 check, per page the old compare found written: was it marked?
/// `CHECKED[0]` pages checked, `[1]` of them unmarked with no sentinel set
/// (a write the marks missed: must stay 0), `[2]` checked while the
/// sentinel was set (nothing to conclude).
pub(crate) static CHECKED: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];

/// Notes a piece the old compare found written (`address`: its title
/// address; `ok`: a page it lies on is marked).
pub(crate) fn check_written(address: u64, ok: bool, reliable: bool) {
    if !on() || BITMAP.get().is_none() {
        return;
    }
    CHECKED[0].fetch_add(1, Ordering::Relaxed);
    if !reliable {
        CHECKED[2].fetch_add(1, Ordering::Relaxed);
    } else if !ok {
        let n = CHECKED[1].fetch_add(1, Ordering::Relaxed);
        if n < 20 {
            tracing::warn!(address = format_args!("{address:#x}"), "write marks: a page found written is not marked");
        }
    }
}

/// For the frame line: pages checked, missed and checked under the sentinel.
pub(crate) fn line() -> Option<String> {
    if !on() {
        return None;
    }
    let [a, b, c] = [0, 1, 2].map(|i| CHECKED[i].load(Ordering::Relaxed));
    let clears = CLEARS.load(Ordering::Relaxed);
    Some(format!("write marks so far: {a} written pages checked, {b} NOT marked (must be 0), {c} not checkable (an unmarked writer ran); bitmap cleared {clears} times; stage 3 left {} MB unscanned", crate::host::MARKS_SKIPPED.load(std::sync::atomic::Ordering::Relaxed) >> 20))
}

/// A piece found written, unmarked while the marks were reliable: how many
/// pages of its copy are marked (none: the copy's views have no or wrong
/// table entries; some: the page arithmetic in the view is off). Logged for
/// the first 40.
pub(crate) fn explain_miss(start: u64, len: u64, piece: u64, n: u64, writer: (u64, u8)) {
    static LOGGED: AtomicU64 = AtomicU64::new(0);
    if LOGGED.fetch_add(1, Ordering::Relaxed) >= 40 {
        return;
    }
    let pages = len.div_ceil(1 << 16).max(1);
    let first = start >> 16;
    let marked_pages = (0..pages.min(1 << 16)).filter(|k| marked((first + k) << 16)).count();
    tracing::warn!(
        copy = format_args!("{start:#x}+{len:#x}"),
        piece = format_args!("{piece:#x}+{n:#x}"),
        marked_pages,
        pages,
        writer = format_args!("{:016x}", writer.0),
        writer_marks = writer.1,
        "write marks: miss detail"
    );
}
