//! Raw Linux syscall wrappers.
//!
//! Thin wrappers around libc functions for socket I/O, epoll, and dynamic loading.
//! No external crates — uses `extern "C"` FFI directly.

use core::ffi::c_void;

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

// Socket
pub const AF_UNIX: i32 = 1;
pub const AF_INET: i32 = 2;
pub const SOCK_STREAM: i32 = 1;
pub const SOCK_DGRAM: i32 = 2;
pub const SOCK_NONBLOCK: i32 = 0o4000;
pub const SOCK_CLOEXEC: i32 = 0o2000000;

// epoll
pub const EPOLL_CTL_ADD: i32 = 1;
pub const EPOLL_CTL_DEL: i32 = 2;
pub const EPOLL_CTL_MOD: i32 = 3;
pub const EPOLL_CLOEXEC: i32 = 0o2000000;

pub const EPOLLIN: u32 = 0x001;
pub const EPOLLOUT: u32 = 0x004;
pub const EPOLLERR: u32 = 0x008;
pub const EPOLLHUP: u32 = 0x010;
pub const EPOLLET: u32 = 1 << 31;
pub const EPOLLONESHOT: u32 = 1 << 30;

// fcntl
pub const F_GETFL: i32 = 3;
pub const F_SETFL: i32 = 4;
pub const O_NONBLOCK: i32 = 0o4000;

// dlopen
pub const RTLD_NOW: i32 = 0x00002;
pub const RTLD_LAZY: i32 = 0x00001;

// ─────────────────────────────────────────────────────────────────────────────
// Structures
// ─────────────────────────────────────────────────────────────────────────────

/// Unix domain socket address (`struct sockaddr_un`).
#[repr(C)]
pub struct SockaddrUn {
    pub sun_family: u16,
    pub sun_path: [u8; 108],
}

impl SockaddrUn {
    /// Create a `sockaddr_un` for the given path bytes.
    pub fn new(path: &[u8]) -> Self {
        let mut addr = SockaddrUn {
            sun_family: AF_UNIX as u16,
            sun_path: [0u8; 108],
        };
        let copy_len = path.len().min(107); // leave room for NUL
        addr.sun_path[..copy_len].copy_from_slice(&path[..copy_len]);
        addr
    }

    /// Size of the address structure including the path.
    pub fn len(path_len: usize) -> u32 {
        // offsetof(sun_path) + path_len + 1 (NUL)
        (2 + path_len.min(107) + 1) as u32
    }
}

/// epoll event structure.
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct EpollEvent {
    pub events: u32,
    pub data: u64,
}

impl EpollEvent {
    pub fn new(events: u32, data: u64) -> Self {
        Self { events, data }
    }

    pub fn empty() -> Self {
        Self { events: 0, data: 0 }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// libc FFI
// ─────────────────────────────────────────────────────────────────────────────

unsafe extern "C" {
    // Socket operations
    pub fn socket(domain: i32, ty: i32, protocol: i32) -> i32;
    pub fn connect(fd: i32, addr: *const SockaddrUn, addrlen: u32) -> i32;
    pub fn bind(fd: i32, addr: *const SockaddrUn, addrlen: u32) -> i32;
    pub fn listen(fd: i32, backlog: i32) -> i32;
    pub fn accept(fd: i32, addr: *mut SockaddrUn, addrlen: *mut u32) -> i32;

    // Basic I/O
    pub fn read(fd: i32, buf: *mut u8, count: usize) -> isize;
    pub fn write(fd: i32, buf: *const u8, count: usize) -> isize;
    pub fn close(fd: i32) -> i32;

    // epoll
    pub fn epoll_create1(flags: i32) -> i32;
    pub fn epoll_ctl(epfd: i32, op: i32, fd: i32, event: *mut EpollEvent) -> i32;
    pub fn epoll_wait(epfd: i32, events: *mut EpollEvent, maxevents: i32, timeout: i32) -> i32;

    // File control
    pub fn fcntl(fd: i32, cmd: i32, ...) -> i32;

    // Dynamic loading
    pub fn dlopen(filename: *const u8, flags: i32) -> *mut c_void;
    pub fn dlsym(handle: *mut c_void, symbol: *const u8) -> *mut c_void;
    pub fn dlclose(handle: *mut c_void) -> i32;
    pub fn dlerror() -> *const u8;

    // errno
    pub fn __errno_location() -> *mut i32;
}

// ─────────────────────────────────────────────────────────────────────────────
// Safe wrappers
// ─────────────────────────────────────────────────────────────────────────────

/// Get the current `errno` value.
pub fn errno() -> i32 {
    unsafe { *__errno_location() }
}

/// Set a file descriptor to non-blocking mode.
pub fn set_nonblocking(fd: i32) -> Result<(), i32> {
    unsafe {
        let flags = fcntl(fd, F_GETFL);
        if flags < 0 {
            return Err(errno());
        }
        let ret = fcntl(fd, F_SETFL, flags | O_NONBLOCK);
        if ret < 0 {
            return Err(errno());
        }
    }
    Ok(())
}

/// Read all available bytes from a file descriptor into a `Vec<u8>`.
/// Returns `Ok(bytes)` on success, `Err(errno)` on failure.
pub fn read_exact(fd: i32, buf: &mut [u8]) -> Result<(), i32> {
    let mut offset = 0;
    while offset < buf.len() {
        let n = unsafe { read(fd, buf[offset..].as_mut_ptr(), buf.len() - offset) };
        if n < 0 {
            return Err(errno());
        }
        if n == 0 {
            return Err(0); // EOF
        }
        offset += n as usize;
    }
    Ok(())
}

/// Write all bytes to a file descriptor.
///
/// Handles `EAGAIN` (errno 11) transparently so this works on both blocking
/// and non-blocking file descriptors.
pub fn write_all(fd: i32, buf: &[u8]) -> Result<(), i32> {
    let mut offset = 0;
    while offset < buf.len() {
        let n = unsafe { write(fd, buf[offset..].as_ptr(), buf.len() - offset) };
        if n < 0 {
            let e = errno();
            if e == 11 {
                // EAGAIN — socket buffer full, yield and retry.
                std::thread::yield_now();
                continue;
            }
            return Err(e);
        }
        if n == 0 {
            return Err(0);
        }
        offset += n as usize;
    }
    Ok(())
}

/// Create a Unix domain stream socket.
pub fn unix_stream_socket() -> Result<i32, i32> {
    let fd = unsafe { socket(AF_UNIX, SOCK_STREAM, 0) };
    if fd < 0 {
        Err(errno())
    } else {
        Ok(fd)
    }
}

/// Connect a socket to a Unix domain address at the given path.
pub fn connect_unix(fd: i32, path: &[u8]) -> Result<(), i32> {
    let addr = SockaddrUn::new(path);
    let len = SockaddrUn::len(path.len());
    let ret = unsafe { connect(fd, &addr, len) };
    if ret < 0 {
        Err(errno())
    } else {
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sockaddr_un_construction() {
        let path = b"/tmp/.X11-unix/X0";
        let addr = SockaddrUn::new(path);
        assert_eq!(addr.sun_family, AF_UNIX as u16);
        assert_eq!(&addr.sun_path[..path.len()], path);
        assert_eq!(addr.sun_path[path.len()], 0); // NUL terminated
    }

    #[test]
    fn sockaddr_un_too_long_path_truncates() {
        let long_path = [b'a'; 200];
        let addr = SockaddrUn::new(&long_path);
        // Should only copy 107 bytes
        assert_eq!(addr.sun_path[106], b'a');
        assert_eq!(addr.sun_path[107], 0);
    }

    #[test]
    fn epoll_event_creation() {
        let ev = EpollEvent::new(EPOLLIN | EPOLLOUT, 42);
        // Copy fields out of packed struct before comparing
        let events = ev.events;
        let data = ev.data;
        assert_eq!(events, EPOLLIN | EPOLLOUT);
        assert_eq!(data, 42);
    }

    #[test]
    fn epoll_event_empty() {
        let ev = EpollEvent::empty();
        let events = ev.events;
        let data = ev.data;
        assert_eq!(events, 0);
        assert_eq!(data, 0);
    }

    #[test]
    fn constants_are_correct() {
        assert_eq!(AF_UNIX, 1);
        assert_eq!(SOCK_STREAM, 1);
        assert_eq!(EPOLL_CTL_ADD, 1);
        assert_eq!(EPOLL_CTL_DEL, 2);
        assert_eq!(EPOLL_CTL_MOD, 3);
        assert_eq!(EPOLLIN, 0x001);
        assert_eq!(EPOLLOUT, 0x004);
    }
}
