//! Sub-decoders for non-standard ABI payloads.
//!
//! [`crate::decode`] handles the standard-ABI portion of calldata; many DeFi
//! protocols additionally pack a sub-format *inside* a `bytes` argument. We
//! classify those sub-formats by **what kind of payload** the bytes carry:
//!
//! | Kind                | Payload shape                                                         | Module             |
//! |---------------------|------------------------------------------------------------------------|--------------------|
//! | recursive           | another standard-ABI calldata (selector + args)                        | [`recurse`]        |
//! | opcode-dispatched   | parallel `(commands, inputs[])` driven by an opcode table              | [`opcode_stream`]  |
//! | packed              | bespoke layout (e.g. `[token20][fee3][token20]…` for V3 paths)         | [`protocols`]      |
//! | enum-tagged         | first word = `kind`, tail decoded per-kind (e.g. Balancer `userData`)  | _todo_             |
//! | caller-dependent    | schema known only to the receiving contract (e.g. V4 `hookData`)       | _todo_             |
//! | opaque              | no schema known and likely no canonical one (raw blob)                 | (graceful default) |
//!
//! Each sub-decoder feeds the orchestrator (web-server) a tree of
//! `DecodeResponse` children so the structure surfaces in the UI. Unknown
//! payloads fall back to opaque hex without further interpretation.

pub mod enum_tagged;
pub mod opcode_stream;
pub mod protocols;
pub mod recurse;
