//! Swift bindings generator, invoked by `scripts/build-rust.sh`.
//!
//! UniFFI's Swift-specific generator runs in "library mode": it reads the type
//! metadata back out of the compiled `libleogit_ffi.a`, so proc-macro exports
//! (which have no UDL file to parse) are always picked up.

fn main() {
    uniffi::uniffi_bindgen_swift();
}
