//! Renderer-independent shell model shared by the D01 renderer spikes.
//!
//! Spike code for `docs/adr/0008-d01-workspace-renderer.md`: the shell context
//! and route stack, the job tray model, bounded persistence, and the synthetic
//! case fixture. Nothing here depends on a window, a renderer, or IPC, and the
//! persisted format is throwaway; it is not the schema proposed for `oia-app`.

pub mod fixture;
pub mod jobs;
pub mod persist;
pub mod shell;
