//! epoll-based I/O reactor.
//!
//! Provides a lightweight event loop primitive for multiplexing file descriptors.

use crate::syscall::{
    self, EpollEvent, EPOLLIN, EPOLLOUT, EPOLLERR, EPOLLHUP,
    EPOLL_CTL_ADD, EPOLL_CTL_DEL, EPOLL_CTL_MOD, EPOLL_CLOEXEC,
};
use std::collections::HashMap;
use std::fmt;

// ─────────────────────────────────────────────────────────────────────────────
// Token
// ─────────────────────────────────────────────────────────────────────────────

/// Opaque token returned when registering a file descriptor.
/// Used to correlate events back to their source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Token(pub u64);

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Token({})", self.0)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Interest
// ─────────────────────────────────────────────────────────────────────────────

/// Interest flags for epoll registration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interest(u32);

impl Interest {
    pub const READABLE: Self = Self(EPOLLIN);
    pub const WRITABLE: Self = Self(EPOLLOUT);
    pub const BOTH: Self = Self(EPOLLIN | EPOLLOUT);

    /// Combine two interest sets.
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Check if readable interest is set.
    pub fn is_readable(self) -> bool {
        self.0 & EPOLLIN != 0
    }

    /// Check if writable interest is set.
    pub fn is_writable(self) -> bool {
        self.0 & EPOLLOUT != 0
    }

    /// Raw epoll flags.
    pub fn bits(self) -> u32 {
        self.0
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ReadyEvent
// ─────────────────────────────────────────────────────────────────────────────

/// An event returned from polling, indicating which I/O operations are ready.
#[derive(Clone, Copy, Debug)]
pub struct ReadyEvent {
    pub token: Token,
    pub readable: bool,
    pub writable: bool,
    pub error: bool,
    pub hangup: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// EpollReactor
// ─────────────────────────────────────────────────────────────────────────────

/// Registration entry tracking the fd for a given token.
struct Registration {
    fd: i32,
    interest: Interest,
}

/// An epoll-based I/O event reactor.
///
/// # Example (conceptual)
/// ```ignore
/// let mut reactor = EpollReactor::new()?;
/// let token = reactor.register(socket_fd, Interest::READABLE)?;
/// let mut events = Vec::new();
/// reactor.poll(&mut events, 1000)?;
/// for ev in &events {
///     if ev.token == token && ev.readable {
///         // socket is readable
///     }
/// }
/// ```
pub struct EpollReactor {
    epfd: i32,
    next_token: u64,
    registrations: HashMap<u64, Registration>,
}

impl EpollReactor {
    /// Create a new epoll reactor.
    pub fn new() -> Result<Self, std::io::Error> {
        let epfd = unsafe { syscall::epoll_create1(EPOLL_CLOEXEC) };
        if epfd < 0 {
            return Err(std::io::Error::from_raw_os_error(syscall::errno()));
        }
        Ok(Self {
            epfd,
            next_token: 1,
            registrations: HashMap::new(),
        })
    }

    /// Register a file descriptor with the given interest.
    /// Returns a unique `Token` for this registration.
    pub fn register(&mut self, fd: i32, interest: Interest) -> Result<Token, std::io::Error> {
        let token = Token(self.next_token);
        self.next_token += 1;

        let mut ev = EpollEvent::new(interest.bits(), token.0);
        let ret = unsafe { syscall::epoll_ctl(self.epfd, EPOLL_CTL_ADD, fd, &mut ev) };
        if ret < 0 {
            return Err(std::io::Error::from_raw_os_error(syscall::errno()));
        }

        self.registrations.insert(token.0, Registration { fd, interest });
        Ok(token)
    }

    /// Modify the interest set for an existing registration.
    pub fn modify(&mut self, token: Token, interest: Interest) -> Result<(), std::io::Error> {
        let reg = self.registrations.get_mut(&token.0)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "token not found"))?;

        let mut ev = EpollEvent::new(interest.bits(), token.0);
        let ret = unsafe { syscall::epoll_ctl(self.epfd, EPOLL_CTL_MOD, reg.fd, &mut ev) };
        if ret < 0 {
            return Err(std::io::Error::from_raw_os_error(syscall::errno()));
        }

        reg.interest = interest;
        Ok(())
    }

    /// Deregister a file descriptor. Does **not** close the fd.
    pub fn deregister(&mut self, token: Token) -> Result<(), std::io::Error> {
        let reg = self.registrations.remove(&token.0)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "token not found"))?;

        let mut ev = EpollEvent::empty();
        let ret = unsafe { syscall::epoll_ctl(self.epfd, EPOLL_CTL_DEL, reg.fd, &mut ev) };
        if ret < 0 {
            return Err(std::io::Error::from_raw_os_error(syscall::errno()));
        }

        Ok(())
    }

    /// Poll for ready events. Blocks up to `timeout_ms` milliseconds
    /// (`-1` = block forever, `0` = return immediately).
    ///
    /// Ready events are appended to `events` (which is **not** cleared first).
    pub fn poll(&self, events: &mut Vec<ReadyEvent>, timeout_ms: i32) -> Result<usize, std::io::Error> {
        const MAX_EVENTS: usize = 64;
        let mut raw = [EpollEvent::empty(); MAX_EVENTS];

        let n = unsafe {
            syscall::epoll_wait(self.epfd, raw.as_mut_ptr(), MAX_EVENTS as i32, timeout_ms)
        };
        if n < 0 {
            let e = syscall::errno();
            // EINTR is not an error — just retry
            if e == 4 {
                return Ok(0);
            }
            return Err(std::io::Error::from_raw_os_error(e));
        }

        let count = n as usize;
        for i in 0..count {
            let ev = raw[i];
            events.push(ReadyEvent {
                token: Token(ev.data),
                readable: ev.events & EPOLLIN != 0,
                writable: ev.events & EPOLLOUT != 0,
                error: ev.events & EPOLLERR != 0,
                hangup: ev.events & EPOLLHUP != 0,
            });
        }

        Ok(count)
    }

    /// Returns the number of registered file descriptors.
    pub fn len(&self) -> usize {
        self.registrations.len()
    }

    /// Returns true if no file descriptors are registered.
    pub fn is_empty(&self) -> bool {
        self.registrations.is_empty()
    }
}

impl Drop for EpollReactor {
    fn drop(&mut self) {
        unsafe {
            syscall::close(self.epfd);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interest_flags() {
        assert!(Interest::READABLE.is_readable());
        assert!(!Interest::READABLE.is_writable());
        assert!(!Interest::WRITABLE.is_readable());
        assert!(Interest::WRITABLE.is_writable());
        assert!(Interest::BOTH.is_readable());
        assert!(Interest::BOTH.is_writable());
    }

    #[test]
    fn interest_union() {
        let combined = Interest::READABLE.union(Interest::WRITABLE);
        assert!(combined.is_readable());
        assert!(combined.is_writable());
        assert_eq!(combined.bits(), Interest::BOTH.bits());
    }

    #[test]
    fn token_display() {
        let t = Token(42);
        assert_eq!(format!("{t}"), "Token(42)");
    }

    #[test]
    fn reactor_creation() {
        let reactor = EpollReactor::new();
        assert!(reactor.is_ok());
        let r = reactor.unwrap();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn reactor_poll_empty_immediate() {
        let reactor = EpollReactor::new().unwrap();
        let mut events = Vec::new();
        // timeout=0 should return immediately with no events
        let n = reactor.poll(&mut events, 0).unwrap();
        assert_eq!(n, 0);
        assert!(events.is_empty());
    }

    #[test]
    fn reactor_register_deregister_pipe() {
        let mut reactor = EpollReactor::new().unwrap();
        // Create a pipe for testing
        let mut fds = [0i32; 2];
        let ret = unsafe { libc_pipe(fds.as_mut_ptr()) };
        assert_eq!(ret, 0);

        let token = reactor.register(fds[0], Interest::READABLE).unwrap();
        assert_eq!(reactor.len(), 1);

        reactor.deregister(token).unwrap();
        assert_eq!(reactor.len(), 0);

        unsafe {
            syscall::close(fds[0]);
            syscall::close(fds[1]);
        }
    }

    unsafe extern "C" {
        #[link_name = "pipe"]
        fn libc_pipe(fds: *mut i32) -> i32;
    }
}
