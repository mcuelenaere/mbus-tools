#![no_main]

use libfuzzer_sys::fuzz_target;
use mbus::Frame;

fuzz_target!(|data: &[u8]| {
    if let Ok(frame) = Frame::from_bytes(data) {
        let _ = frame.to_bytes();
    }
});
