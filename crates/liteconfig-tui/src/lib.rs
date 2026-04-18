//! Library surface of the TUI crate — exposed so integration tests (and
//! future embedders) can drive the `App` state machine without pulling in the
//! binary entrypoint.

pub mod app;
pub mod events;
pub mod theme;
pub mod ui;
