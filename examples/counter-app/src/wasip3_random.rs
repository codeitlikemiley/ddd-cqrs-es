//! `getrandom` 0.3 bridge to final WASI `wasi:random/random@0.3.0`.

use getrandom03::Error;

const MAX_HOST_CHUNK: usize = 64 * 1024;

/// Custom backend selected by `.cargo/config.toml` for the experimental
/// `wasm32-wasip3` Rust target canary.
///
/// # Safety
///
/// `getrandom` guarantees that `destination` points to a writable allocation
/// of exactly `length` bytes. This function initializes and fills every byte.
#[unsafe(no_mangle)]
unsafe extern "Rust" fn __getrandom_v03_custom(
    destination: *mut u8,
    length: usize,
) -> Result<(), Error> {
    if length == 0 {
        return Ok(());
    }
    if destination.is_null() {
        return Err(Error::UNEXPECTED);
    }
    // SAFETY: guaranteed by the custom-backend caller contract above.
    let output = unsafe {
        core::ptr::write_bytes(destination, 0, length);
        core::slice::from_raw_parts_mut(destination, length)
    };
    let mut offset = 0;
    while offset < output.len() {
        let requested = (output.len() - offset).min(MAX_HOST_CHUNK);
        let random = wasip3::random::random::get_random_bytes(requested as u64);
        if random.is_empty() || random.len() > requested {
            output.fill(0);
            return Err(Error::UNEXPECTED);
        }
        let end = offset + random.len();
        output[offset..end].copy_from_slice(&random);
        offset = end;
    }
    Ok(())
}
