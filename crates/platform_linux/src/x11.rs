//! X11 protocol implementation over Unix domain sockets.
//!
//! Implements the core X11 wire protocol: connection setup, window creation,
//! event reading, atom interning, and WM_DELETE_WINDOW handling.

use crate::syscall;
use common::{BufWriter, Cursor, Endian, ParseError};
use std::fmt;

// ─────────────────────────────────────────────────────────────────────────────
// Type aliases
// ─────────────────────────────────────────────────────────────────────────────

pub type Window = u32;
pub type Atom = u32;
pub type VisualId = u32;
pub type Colormap = u32;
pub type Drawable = u32;
pub type GContext = u32;
pub type Pixmap = u32;

// ─────────────────────────────────────────────────────────────────────────────
// Opcodes
// ─────────────────────────────────────────────────────────────────────────────

pub const OPCODE_CREATE_WINDOW: u8 = 1;
pub const OPCODE_CHANGE_WINDOW_ATTRIBUTES: u8 = 2;
pub const OPCODE_GET_WINDOW_ATTRIBUTES: u8 = 3;
pub const OPCODE_DESTROY_WINDOW: u8 = 4;
pub const OPCODE_MAP_WINDOW: u8 = 8;
pub const OPCODE_UNMAP_WINDOW: u8 = 10;
pub const OPCODE_INTERN_ATOM: u8 = 16;
pub const OPCODE_CHANGE_PROPERTY: u8 = 18;
pub const OPCODE_CREATE_GC: u8 = 55;
pub const OPCODE_PUT_IMAGE: u8 = 72;

// Event codes (low 7 bits)
pub const EVENT_KEY_PRESS: u8 = 2;
pub const EVENT_KEY_RELEASE: u8 = 3;
pub const EVENT_BUTTON_PRESS: u8 = 4;
pub const EVENT_BUTTON_RELEASE: u8 = 5;
pub const EVENT_MOTION_NOTIFY: u8 = 6;
pub const EVENT_EXPOSE: u8 = 12;
pub const EVENT_MAP_NOTIFY: u8 = 19;
pub const EVENT_UNMAP_NOTIFY: u8 = 18;
pub const EVENT_DESTROY_NOTIFY: u8 = 17;
pub const EVENT_CONFIGURE_NOTIFY: u8 = 22;
pub const EVENT_CLIENT_MESSAGE: u8 = 33;
pub const EVENT_FOCUS_IN: u8 = 9;
pub const EVENT_FOCUS_OUT: u8 = 10;

// Window attributes value masks
pub const CW_BACK_PIXEL: u32 = 1 << 1;
pub const CW_EVENT_MASK: u32 = 1 << 11;
pub const CW_COLORMAP: u32 = 1 << 13;

// Event masks
pub const EVENT_MASK_KEY_PRESS: u32 = 1 << 0;
pub const EVENT_MASK_KEY_RELEASE: u32 = 1 << 1;
pub const EVENT_MASK_BUTTON_PRESS: u32 = 1 << 2;
pub const EVENT_MASK_BUTTON_RELEASE: u32 = 1 << 3;
pub const EVENT_MASK_POINTER_MOTION: u32 = 1 << 6;
pub const EVENT_MASK_EXPOSURE: u32 = 1 << 15;
pub const EVENT_MASK_STRUCTURE_NOTIFY: u32 = 1 << 17;
pub const EVENT_MASK_FOCUS_CHANGE: u32 = 1 << 21;

pub const DEFAULT_EVENT_MASK: u32 = EVENT_MASK_KEY_PRESS
    | EVENT_MASK_KEY_RELEASE
    | EVENT_MASK_BUTTON_PRESS
    | EVENT_MASK_BUTTON_RELEASE
    | EVENT_MASK_POINTER_MOTION
    | EVENT_MASK_EXPOSURE
    | EVENT_MASK_STRUCTURE_NOTIFY
    | EVENT_MASK_FOCUS_CHANGE;

// Property modes
pub const PROP_MODE_REPLACE: u8 = 0;
pub const PROP_MODE_PREPEND: u8 = 1;
pub const PROP_MODE_APPEND: u8 = 2;

// ─────────────────────────────────────────────────────────────────────────────
// X11Error
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum X11Error {
    Io(i32),
    Parse(ParseError),
    Protocol(&'static str),
    ConnectionRefused,
    SetupFailed { reason: String },
    ServerError {
        code: u8,
        major_opcode: u8,
        minor_opcode: u16,
        resource_id: u32,
    },
}

impl fmt::Display for X11Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "X11 I/O error: errno {e}"),
            Self::Parse(e) => write!(f, "X11 parse error: {e}"),
            Self::Protocol(msg) => write!(f, "X11 protocol error: {msg}"),
            Self::ConnectionRefused => write!(f, "X11 connection refused"),
            Self::SetupFailed { reason } => write!(f, "X11 setup failed: {reason}"),
            Self::ServerError { code, major_opcode, minor_opcode, resource_id } => {
                write!(f, "X11 server error: code={code} major={major_opcode} minor={minor_opcode} rid={resource_id}")
            }
        }
    }
}

impl From<ParseError> for X11Error {
    fn from(e: ParseError) -> Self {
        Self::Parse(e)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Setup structures
// ─────────────────────────────────────────────────────────────────────────────

/// X11 connection setup request (sent by client).
pub struct SetupRequest {
    pub byte_order: u8,
    pub major_version: u16,
    pub minor_version: u16,
    pub auth_proto_name: Vec<u8>,
    pub auth_proto_data: Vec<u8>,
}

impl SetupRequest {
    /// Create a default setup request (little-endian, X11 version 11.0, no auth).
    pub fn default_le() -> Self {
        Self {
            byte_order: 0x6c, // little-endian
            major_version: 11,
            minor_version: 0,
            auth_proto_name: Vec::new(),
            auth_proto_data: Vec::new(),
        }
    }

    /// Serialize the setup request to bytes.
    pub fn serialize(&self) -> Vec<u8> {
        let mut w = BufWriter::new(Endian::Little);
        w.u8(self.byte_order);
        w.u8(0); // unused
        w.u16(self.major_version);
        w.u16(self.minor_version);
        w.u16(self.auth_proto_name.len() as u16);
        w.u16(self.auth_proto_data.len() as u16);
        w.u16(0); // unused
        w.bytes(&self.auth_proto_name);
        w.pad4();
        w.bytes(&self.auth_proto_data);
        w.pad4();
        w.finish()
    }
}

/// Parsed X11 setup success response.
pub struct SetupResponse {
    pub resource_id_base: u32,
    pub resource_id_mask: u32,
    pub root_window: Window,
    pub root_visual: VisualId,
    pub root_depth: u8,
    pub width_pixels: u16,
    pub height_pixels: u16,
    pub min_keycode: u8,
    pub max_keycode: u8,
}

// ─────────────────────────────────────────────────────────────────────────────
// X11Event
// ─────────────────────────────────────────────────────────────────────────────

/// Parsed X11 event (all X11 events are 32 bytes on the wire).
#[derive(Clone, Debug)]
pub enum X11Event {
    KeyPress {
        keycode: u8,
        x: i16,
        y: i16,
        state: u16,
        window: Window,
    },
    KeyRelease {
        keycode: u8,
        x: i16,
        y: i16,
        state: u16,
        window: Window,
    },
    ButtonPress {
        button: u8,
        x: i16,
        y: i16,
        state: u16,
        window: Window,
    },
    ButtonRelease {
        button: u8,
        x: i16,
        y: i16,
        state: u16,
        window: Window,
    },
    MotionNotify {
        x: i16,
        y: i16,
        state: u16,
        window: Window,
    },
    Expose {
        window: Window,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        count: u16,
    },
    ConfigureNotify {
        window: Window,
        x: i16,
        y: i16,
        width: u16,
        height: u16,
    },
    ClientMessage {
        window: Window,
        type_atom: Atom,
        data: [u8; 20],
    },
    MapNotify {
        window: Window,
    },
    UnmapNotify {
        window: Window,
    },
    DestroyNotify {
        window: Window,
    },
    FocusIn {
        window: Window,
    },
    FocusOut {
        window: Window,
    },
    Unknown {
        code: u8,
        data: [u8; 32],
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// X11Connection
// ─────────────────────────────────────────────────────────────────────────────

/// A connection to an X11 server over a Unix domain socket.
pub struct X11Connection {
    pub fd: i32,
    pub endian: Endian,
    pub sequence: u16,
    pub resource_id_base: u32,
    pub resource_id_mask: u32,
    next_rid: u32,
    pub root_window: Window,
    pub root_visual: VisualId,
    pub root_depth: u8,
    pub screen_width: u16,
    pub screen_height: u16,
}

impl X11Connection {
    /// Connect to the X11 server on display `display_num`.
    /// Connects to `/tmp/.X11-unix/X{display_num}`.
    pub fn connect(display_num: u32) -> Result<Self, X11Error> {
        // Build socket path
        let path = format!("/tmp/.X11-unix/X{display_num}");
        let path_bytes = path.as_bytes();

        // Create Unix domain socket
        let fd = syscall::unix_stream_socket().map_err(X11Error::Io)?;

        // Connect to X server
        syscall::connect_unix(fd, path_bytes).map_err(|e| {
            unsafe { syscall::close(fd); }
            if e == 111 {
                X11Error::ConnectionRefused
            } else {
                X11Error::Io(e)
            }
        })?;

        // Send setup request
        let setup_req = SetupRequest::default_le();
        let req_bytes = setup_req.serialize();
        syscall::write_all(fd, &req_bytes).map_err(|e| {
            unsafe { syscall::close(fd); }
            X11Error::Io(e)
        })?;

        // Read setup response header (8 bytes to determine status + length)
        let mut header = [0u8; 8];
        syscall::read_exact(fd, &mut header).map_err(|e| {
            unsafe { syscall::close(fd); }
            X11Error::Io(e)
        })?;

        let status = header[0];
        if status == 0 {
            // Failed
            let reason_len = header[1] as usize;
            let additional_len = u16::from_le_bytes([header[6], header[7]]) as usize * 4;
            let mut additional = vec![0u8; additional_len];
            let _ = syscall::read_exact(fd, &mut additional);
            unsafe { syscall::close(fd); }
            let reason = String::from_utf8_lossy(&additional[..reason_len.min(additional.len())]).to_string();
            return Err(X11Error::SetupFailed { reason });
        }
        if status == 2 {
            // Authenticate — not supported
            unsafe { syscall::close(fd); }
            return Err(X11Error::Protocol("authentication required but not supported"));
        }
        // status == 1 → Success

        // Read the rest of the setup response
        let additional_len = u16::from_le_bytes([header[6], header[7]]) as usize * 4;
        let mut resp_data = vec![0u8; additional_len];
        syscall::read_exact(fd, &mut resp_data).map_err(|e| {
            unsafe { syscall::close(fd); }
            X11Error::Io(e)
        })?;

        // Parse the response
        let mut c = Cursor::new(&resp_data, Endian::Little);

        // Fixed fields after the 8-byte header
        // release_number(4), resource_id_base(4), resource_id_mask(4),
        // motion_buffer_size(4), vendor_length(2), max_request_length(2),
        // num_screens(1), num_pixmap_formats(1), ...
        let _release = c.u32()?;
        let resource_id_base = c.u32()?;
        let resource_id_mask = c.u32()?;
        let _motion_buf_size = c.u32()?;
        let vendor_len = c.u16()? as usize;
        let _max_req_len = c.u16()?;
        let num_screens = c.u8()?;
        let num_formats = c.u8()?;
        let _image_byte_order = c.u8()?;
        let _bitmap_bit_order = c.u8()?;
        let _bitmap_scanline_unit = c.u8()?;
        let _bitmap_scanline_pad = c.u8()?;
        let min_keycode = c.u8()?;
        let max_keycode = c.u8()?;
        c.skip(4)?; // unused

        // Vendor string (padded to 4)
        c.skip(vendor_len)?;
        let vendor_pad = (4 - (vendor_len % 4)) % 4;
        c.skip(vendor_pad)?;

        // Pixmap formats (8 bytes each)
        c.skip(num_formats as usize * 8)?;

        // Parse first screen
        if num_screens == 0 {
            unsafe { syscall::close(fd); }
            return Err(X11Error::Protocol("no screens available"));
        }

        let root_window = c.u32()?;
        let _default_colormap = c.u32()?;
        let _white_pixel = c.u32()?;
        let _black_pixel = c.u32()?;
        let _current_input_masks = c.u32()?;
        let width_pixels = c.u16()?;
        let height_pixels = c.u16()?;
        let _width_mm = c.u16()?;
        let _height_mm = c.u16()?;
        let _min_installed_maps = c.u16()?;
        let _max_installed_maps = c.u16()?;
        let root_visual = c.u32()?;
        let _backing_stores = c.u8()?;
        let _save_unders = c.u8()?;
        let root_depth = c.u8()?;
        let _num_depths = c.u8()?;

        let _ = min_keycode;
        let _ = max_keycode;

        Ok(X11Connection {
            fd,
            endian: Endian::Little,
            sequence: 0,
            resource_id_base,
            resource_id_mask,
            next_rid: 1,
            root_window,
            root_visual,
            root_depth,
            screen_width: width_pixels,
            screen_height: height_pixels,
        })
    }

    /// Allocate a new X11 resource ID.
    pub fn alloc_id(&mut self) -> u32 {
        let id = self.resource_id_base | (self.next_rid & self.resource_id_mask);
        self.next_rid = self.next_rid.wrapping_add(1);
        id
    }

    /// Send a raw X11 request (must already be properly formatted).
    pub fn send_request(&mut self, data: &[u8]) -> Result<u16, X11Error> {
        syscall::write_all(self.fd, data).map_err(X11Error::Io)?;
        self.sequence = self.sequence.wrapping_add(1);
        Ok(self.sequence)
    }

    /// Create a window on the root window.
    pub fn create_window(&mut self, width: u16, height: u16) -> Result<Window, X11Error> {
        let wid = self.alloc_id();
        let x: i16 = 0;
        let y: i16 = 0;
        let border_width: u16 = 0;
        let class: u16 = 1; // InputOutput
        let visual = self.root_visual;
        let depth = self.root_depth;

        // Value mask: BackPixel + EventMask
        let value_mask: u32 = CW_BACK_PIXEL | CW_EVENT_MASK;
        let num_values: u16 = 2;

        let request_len: u16 = 8 + num_values; // in 4-byte units

        let mut w = BufWriter::new(Endian::Little);
        w.u8(OPCODE_CREATE_WINDOW);
        w.u8(depth);
        w.u16(request_len);
        w.u32(wid);
        w.u32(self.root_window);
        w.u16(x as u16);
        w.u16(y as u16);
        w.u16(width);
        w.u16(height);
        w.u16(border_width);
        w.u16(class);
        w.u32(visual);
        w.u32(value_mask);
        // Values (in bit order of mask)
        w.u32(0x00000000); // back_pixel = black
        w.u32(DEFAULT_EVENT_MASK);

        self.send_request(&w.finish())?;
        Ok(wid)
    }

    /// Map (show) a window.
    pub fn map_window(&mut self, window: Window) -> Result<(), X11Error> {
        let mut w = BufWriter::new(Endian::Little);
        w.u8(OPCODE_MAP_WINDOW);
        w.u8(0); // unused
        w.u16(2); // request length in 4-byte units
        w.u32(window);
        self.send_request(&w.finish())?;
        Ok(())
    }

    /// Unmap (hide) a window.
    pub fn unmap_window(&mut self, window: Window) -> Result<(), X11Error> {
        let mut w = BufWriter::new(Endian::Little);
        w.u8(OPCODE_UNMAP_WINDOW);
        w.u8(0);
        w.u16(2);
        w.u32(window);
        self.send_request(&w.finish())?;
        Ok(())
    }

    /// Destroy a window.
    pub fn destroy_window(&mut self, window: Window) -> Result<(), X11Error> {
        let mut w = BufWriter::new(Endian::Little);
        w.u8(OPCODE_DESTROY_WINDOW);
        w.u8(0);
        w.u16(2);
        w.u32(window);
        self.send_request(&w.finish())?;
        Ok(())
    }

    /// Intern an atom (get or create a named atom).
    /// Returns the sequence number; caller must read the reply.
    pub fn intern_atom(&mut self, name: &str, only_if_exists: bool) -> Result<u16, X11Error> {
        let name_bytes = name.as_bytes();
        let name_pad = (4 - (name_bytes.len() % 4)) % 4;
        let request_len: u16 = (8 + name_bytes.len() + name_pad) as u16 / 4;

        let mut w = BufWriter::new(Endian::Little);
        w.u8(OPCODE_INTERN_ATOM);
        w.u8(if only_if_exists { 1 } else { 0 });
        w.u16(request_len);
        w.u16(name_bytes.len() as u16);
        w.u16(0); // unused
        w.bytes(name_bytes);
        for _ in 0..name_pad {
            w.u8(0);
        }

        self.send_request(&w.finish())
    }

    /// Read the reply to an InternAtom request.
    pub fn read_intern_atom_reply(&mut self) -> Result<Atom, X11Error> {
        let mut buf = [0u8; 32];
        syscall::read_exact(self.fd, &mut buf).map_err(X11Error::Io)?;

        // Reply type should be 1
        if buf[0] != 1 {
            return Err(X11Error::Protocol("expected reply, got event or error"));
        }

        let atom = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
        Ok(atom)
    }

    /// Change a property on a window.
    pub fn change_property(
        &mut self,
        window: Window,
        property: Atom,
        type_atom: Atom,
        format: u8,
        data: &[u8],
    ) -> Result<(), X11Error> {
        let data_pad = (4 - (data.len() % 4)) % 4;
        let data_len_units = match format {
            8 => data.len(),
            16 => data.len() / 2,
            32 => data.len() / 4,
            _ => return Err(X11Error::Protocol("invalid property format")),
        };
        let request_len: u16 = (24 + data.len() + data_pad) as u16 / 4;

        let mut w = BufWriter::new(Endian::Little);
        w.u8(OPCODE_CHANGE_PROPERTY);
        w.u8(PROP_MODE_REPLACE);
        w.u16(request_len);
        w.u32(window);
        w.u32(property);
        w.u32(type_atom);
        w.u8(format);
        w.u8(0); w.u8(0); w.u8(0); // pad
        w.u32(data_len_units as u32);
        w.bytes(data);
        for _ in 0..data_pad {
            w.u8(0);
        }

        self.send_request(&w.finish())?;
        Ok(())
    }

    /// Set WM_PROTOCOLS for WM_DELETE_WINDOW support.
    pub fn set_wm_protocols(&mut self, window: Window, wm_protocols: Atom, atoms: &[Atom]) -> Result<(), X11Error> {
        let data: Vec<u8> = atoms.iter().flat_map(|a| a.to_le_bytes()).collect();
        self.change_property(window, wm_protocols, 4 /* XA_ATOM */, 32, &data)
    }

    /// Read a single X11 event (blocking).
    pub fn read_event(&mut self) -> Result<X11Event, X11Error> {
        let mut buf = [0u8; 32];
        syscall::read_exact(self.fd, &mut buf).map_err(X11Error::Io)?;

        let code = buf[0] & 0x7f; // strip "sent" flag
        let mut c = Cursor::new(&buf, Endian::Little);
        c.skip(1).unwrap(); // skip code byte

        match code {
            0 => {
                // Error
                let error_code = buf[1];
                let _seq = u16::from_le_bytes([buf[2], buf[3]]);
                let resource_id = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
                let minor_opcode = u16::from_le_bytes([buf[8], buf[9]]);
                let major_opcode = buf[10];
                Err(X11Error::ServerError {
                    code: error_code,
                    major_opcode,
                    minor_opcode,
                    resource_id,
                })
            }
            EVENT_KEY_PRESS | EVENT_KEY_RELEASE => {
                let keycode = buf[1];
                let _seq = u16::from_le_bytes([buf[2], buf[3]]);
                let _time = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
                let _root = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
                let window = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
                let _child = u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);
                let _root_x = i16::from_le_bytes([buf[20], buf[21]]);
                let _root_y = i16::from_le_bytes([buf[22], buf[23]]);
                let x = i16::from_le_bytes([buf[24], buf[25]]);
                let y = i16::from_le_bytes([buf[26], buf[27]]);
                let state = u16::from_le_bytes([buf[28], buf[29]]);

                if code == EVENT_KEY_PRESS {
                    Ok(X11Event::KeyPress { keycode, x, y, state, window })
                } else {
                    Ok(X11Event::KeyRelease { keycode, x, y, state, window })
                }
            }
            EVENT_BUTTON_PRESS | EVENT_BUTTON_RELEASE => {
                let button = buf[1];
                let window = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
                let x = i16::from_le_bytes([buf[24], buf[25]]);
                let y = i16::from_le_bytes([buf[26], buf[27]]);
                let state = u16::from_le_bytes([buf[28], buf[29]]);

                if code == EVENT_BUTTON_PRESS {
                    Ok(X11Event::ButtonPress { button, x, y, state, window })
                } else {
                    Ok(X11Event::ButtonRelease { button, x, y, state, window })
                }
            }
            EVENT_MOTION_NOTIFY => {
                let window = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
                let x = i16::from_le_bytes([buf[24], buf[25]]);
                let y = i16::from_le_bytes([buf[26], buf[27]]);
                let state = u16::from_le_bytes([buf[28], buf[29]]);
                Ok(X11Event::MotionNotify { x, y, state, window })
            }
            EVENT_EXPOSE => {
                let window = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
                let x = u16::from_le_bytes([buf[8], buf[9]]);
                let y = u16::from_le_bytes([buf[10], buf[11]]);
                let width = u16::from_le_bytes([buf[12], buf[13]]);
                let height = u16::from_le_bytes([buf[14], buf[15]]);
                let count = u16::from_le_bytes([buf[16], buf[17]]);
                Ok(X11Event::Expose { window, x, y, width, height, count })
            }
            EVENT_CONFIGURE_NOTIFY => {
                let window = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
                let x = i16::from_le_bytes([buf[12], buf[13]]);
                let y = i16::from_le_bytes([buf[14], buf[15]]);
                let width = u16::from_le_bytes([buf[16], buf[17]]);
                let height = u16::from_le_bytes([buf[18], buf[19]]);
                Ok(X11Event::ConfigureNotify { window, x, y, width, height })
            }
            EVENT_CLIENT_MESSAGE => {
                let _format = buf[1];
                let window = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
                let type_atom = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
                let mut data = [0u8; 20];
                data.copy_from_slice(&buf[12..32]);
                Ok(X11Event::ClientMessage { window, type_atom, data })
            }
            EVENT_MAP_NOTIFY => {
                let window = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
                Ok(X11Event::MapNotify { window })
            }
            EVENT_UNMAP_NOTIFY => {
                let window = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
                Ok(X11Event::UnmapNotify { window })
            }
            EVENT_DESTROY_NOTIFY => {
                let window = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
                Ok(X11Event::DestroyNotify { window })
            }
            EVENT_FOCUS_IN => {
                let window = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
                Ok(X11Event::FocusIn { window })
            }
            EVENT_FOCUS_OUT => {
                let window = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
                Ok(X11Event::FocusOut { window })
            }
            _ => {
                Ok(X11Event::Unknown { code, data: buf })
            }
        }
    }
}

impl Drop for X11Connection {
    fn drop(&mut self) {
        unsafe {
            syscall::close(self.fd);
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
    fn setup_request_serialization() {
        let req = SetupRequest::default_le();
        let bytes = req.serialize();
        assert_eq!(bytes[0], 0x6c); // little-endian
        assert_eq!(bytes[1], 0);    // unused
        // major version = 11 (LE)
        assert_eq!(bytes[2], 11);
        assert_eq!(bytes[3], 0);
        // minor version = 0
        assert_eq!(bytes[4], 0);
        assert_eq!(bytes[5], 0);
        // auth name len = 0
        assert_eq!(bytes[6], 0);
        assert_eq!(bytes[7], 0);
        assert_eq!(bytes.len(), 12);
    }

    #[test]
    fn alloc_id_increments() {
        // Simulate connection state
        let mut conn = X11Connection {
            fd: -1,
            endian: Endian::Little,
            sequence: 0,
            resource_id_base: 0x0400_0000,
            resource_id_mask: 0x001F_FFFF,
            next_rid: 1,
            root_window: 0,
            root_visual: 0,
            root_depth: 24,
            screen_width: 1920,
            screen_height: 1080,
        };

        let id1 = conn.alloc_id();
        let id2 = conn.alloc_id();
        let id3 = conn.alloc_id();

        assert_eq!(id1, 0x0400_0001);
        assert_eq!(id2, 0x0400_0002);
        assert_eq!(id3, 0x0400_0003);

        // Don't close fd=-1
        conn.fd = -1;
        std::mem::forget(conn); // avoid Drop closing fd=-1
    }

    #[test]
    fn event_constants() {
        assert_eq!(EVENT_KEY_PRESS, 2);
        assert_eq!(EVENT_KEY_RELEASE, 3);
        assert_eq!(EVENT_BUTTON_PRESS, 4);
        assert_eq!(EVENT_EXPOSE, 12);
        assert_eq!(EVENT_CONFIGURE_NOTIFY, 22);
        assert_eq!(EVENT_CLIENT_MESSAGE, 33);
    }

    #[test]
    fn opcode_constants() {
        assert_eq!(OPCODE_CREATE_WINDOW, 1);
        assert_eq!(OPCODE_MAP_WINDOW, 8);
        assert_eq!(OPCODE_INTERN_ATOM, 16);
        assert_eq!(OPCODE_CHANGE_PROPERTY, 18);
    }

    #[test]
    fn x11_error_display() {
        let e = X11Error::Protocol("bad window");
        assert_eq!(format!("{e}"), "X11 protocol error: bad window");

        let e = X11Error::ConnectionRefused;
        assert_eq!(format!("{e}"), "X11 connection refused");
    }
}
