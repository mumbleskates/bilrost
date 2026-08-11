//! These are the modules where we stash the impls for the supported types. Many common types that
//! are found in the language may be supported by multiple encoders, and so they have a lot of their
//! actual encoding implementations in their respective encoding modules (like encoding::general,
//! encoding::plain_bytes, etc.). Those types still have common definitions of what constitutes an
//! "empty" value, or they may implement different types of homogenous collections, so those parts
//! of their implementations are found here, in the always-enabled modules.
//!
//! Third-party types are conditionally enabled by feature, and all of the associated functionality
//! that we provide is defined in each of those modules, including for std.

mod additional;
mod core_and_alloc;
mod primitives;

mod common;

#[cfg(feature = "arrayvec")]
mod arrayvec;
#[cfg(feature = "bstr")]
mod bstr;
#[cfg(feature = "bytestring")]
mod bytestring;
#[cfg(feature = "chrono")]
mod chrono;
#[cfg(feature = "hashbrown")]
mod hashbrown;
#[cfg(feature = "jiff")]
mod jiff;
#[cfg(feature = "smallvec")]
mod smallvec;
#[cfg(feature = "smol_str")]
mod smol_str;
#[cfg(feature = "std")]
mod std;
#[cfg(feature = "thin-vec")]
mod thin_vec;
#[cfg(feature = "time")]
mod time;
#[cfg(feature = "tinyvec")]
mod tinyvec;
