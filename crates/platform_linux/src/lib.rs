//! # Platform Linux
//!
//! Low-level Linux platform layer for the browser engine.
//! Provides X11 window management, epoll-based I/O reactor, and Vulkan GPU loader.
//!
//! **Zero external crates** — uses raw syscalls via `libc` FFI.

pub mod syscall;
pub mod epoll;
pub mod x11;
pub mod vulkan;

