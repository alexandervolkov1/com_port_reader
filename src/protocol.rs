//! Wire protocols for supported physical and virtual instruments.
//!
//! The Metakon module implements device frames; the virtual-instrument module defines a framed
//! request/response protocol shared by serial and in-memory transports.

#[path = "protocol/metakon.rs"]
pub mod metakon;

#[path = "protocol/virtual_instrument.rs"]
pub mod virtual_instrument;
