//! The main stack's high-water mark: the unused stack is filled with a pattern at
//! boot, and the deepest overwritten word shows how much was ever used.

/// The fill pattern.
const PAINT: u32 = 0xA5A5_A5A5;
/// Left alone above the stack's end: esp-hal's guard word sits 60 bytes in.
const GUARD_MARGIN: usize = 128;
/// Left alone below the painting function's own frame.
const FRAME_MARGIN: usize = 512;

#[allow(unsafe_code, reason = "the stack bounds are linker symbols")]
unsafe extern "C" {
    static _stack_start_cpu0: u8;
    static _stack_end_cpu0: u8;
}

/// The stack's lowest and highest addresses.
fn bounds() -> (usize, usize) {
    (
        (&raw const _stack_end_cpu0) as usize,
        (&raw const _stack_start_cpu0) as usize,
    )
}

/// Fills the unused stack below the caller. Call it first thing in `main`.
#[allow(
    unsafe_code,
    reason = "fills unused stack memory below the current frame"
)]
pub fn paint() {
    let (end, _) = bounds();
    let probe = 0u8;
    let top = (&raw const probe) as usize - FRAME_MARGIN;
    let mut addr = (end + GUARD_MARGIN + 3) & !3;
    while addr + 4 <= top {
        // SAFETY: the words between the guard margin and below the current
        // frame are unused stack, so no live data is overwritten.
        unsafe { core::ptr::write_volatile(addr as *mut u32, PAINT) };
        addr += 4;
    }
}

/// The most stack ever used, and the stack's size, in bytes.
#[allow(unsafe_code, reason = "reads stack memory to find the painted words")]
pub fn stack_high_water() -> (usize, usize) {
    let (end, start) = bounds();
    let mut addr = (end + GUARD_MARGIN + 3) & !3;
    // SAFETY: reads only, within the stack's bounds.
    while addr < start && unsafe { core::ptr::read_volatile(addr as *const u32) } == PAINT {
        addr += 4;
    }
    (start - addr, start - end)
}
