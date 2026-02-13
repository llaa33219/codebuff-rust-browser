//! X11 keycode → key event mapping.
//!
//! Converts raw X11 keycodes (evdev-based) to semantic `KeyEvent` values.
//! Handles Shift and Control modifiers for a US QWERTY keyboard layout.

// ─────────────────────────────────────────────────────────────────────────────
// KeyEvent
// ─────────────────────────────────────────────────────────────────────────────

/// A semantic key event produced from an X11 keycode and modifier state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyEvent {
    /// A printable character.
    Char(char),
    Backspace,
    Delete,
    Enter,
    Escape,
    Tab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    /// A Control+key combination (the character is lowercase).
    Ctrl(char),
    /// A function key (F1–F12).
    F(u8),
    /// An unrecognised keycode.
    Unknown(u8),
}

// ─────────────────────────────────────────────────────────────────────────────
// Modifier masks
// ─────────────────────────────────────────────────────────────────────────────

const SHIFT_MASK: u16 = 1 << 0;
const CONTROL_MASK: u16 = 1 << 2;

// ─────────────────────────────────────────────────────────────────────────────
// Keycode tables  (evdev keycodes — standard for modern Linux / X.Org)
// ─────────────────────────────────────────────────────────────────────────────

/// Map of keycodes 10–61 to `(normal_char, shifted_char)`.
const PRINTABLE: [(u8, char, char); 47] = [
    // Row 1: number row (keycodes 10–21)
    (10, '1', '!'),
    (11, '2', '@'),
    (12, '3', '#'),
    (13, '4', '$'),
    (14, '5', '%'),
    (15, '6', '^'),
    (16, '7', '&'),
    (17, '8', '*'),
    (18, '9', '('),
    (19, '0', ')'),
    (20, '-', '_'),
    (21, '=', '+'),
    // Row 2: QWERTY (keycodes 24–35)
    (24, 'q', 'Q'),
    (25, 'w', 'W'),
    (26, 'e', 'E'),
    (27, 'r', 'R'),
    (28, 't', 'T'),
    (29, 'y', 'Y'),
    (30, 'u', 'U'),
    (31, 'i', 'I'),
    (32, 'o', 'O'),
    (33, 'p', 'P'),
    (34, '[', '{'),
    (35, ']', '}'),
    // Row 3: ASDF (keycodes 38–48)
    (38, 'a', 'A'),
    (39, 's', 'S'),
    (40, 'd', 'D'),
    (41, 'f', 'F'),
    (42, 'g', 'G'),
    (43, 'h', 'H'),
    (44, 'j', 'J'),
    (45, 'k', 'K'),
    (46, 'l', 'L'),
    (47, ';', ':'),
    (48, '\'', '"'),
    // Row 4: ZXCV (keycodes 49–61)
    (49, '`', '~'),
    (51, '\\', '|'),
    (52, 'z', 'Z'),
    (53, 'x', 'X'),
    (54, 'c', 'C'),
    (55, 'v', 'V'),
    (56, 'b', 'B'),
    (57, 'n', 'N'),
    (58, 'm', 'M'),
    (59, ',', '<'),
    (60, '.', '>'),
    (61, '/', '?'),
];

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Convert an X11 keycode and modifier state into a [`KeyEvent`].
///
/// The `state` parameter carries X11 modifier bits:
/// - bit 0 → Shift
/// - bit 2 → Control
pub fn keycode_to_event(keycode: u8, state: u16) -> KeyEvent {
    let shift = state & SHIFT_MASK != 0;
    let ctrl = state & CONTROL_MASK != 0;

    // Special keys (independent of modifiers)
    match keycode {
        9 => return KeyEvent::Escape,
        22 => return KeyEvent::Backspace,
        23 => return KeyEvent::Tab,
        36 => return KeyEvent::Enter,
        110 => return KeyEvent::Home,
        111 => return KeyEvent::Up,
        112 => return KeyEvent::PageUp,
        113 => return KeyEvent::Left,
        114 => return KeyEvent::Right,
        115 => return KeyEvent::End,
        116 => return KeyEvent::Down,
        117 => return KeyEvent::PageDown,
        119 => return KeyEvent::Delete,
        _ => {}
    }

    // Function keys
    if keycode >= 67 && keycode <= 76 {
        return KeyEvent::F((keycode - 67 + 1) as u8);
    }
    if keycode == 95 {
        return KeyEvent::F(11);
    }
    if keycode == 96 {
        return KeyEvent::F(12);
    }

    // Space
    if keycode == 65 {
        if ctrl {
            return KeyEvent::Ctrl(' ');
        }
        return KeyEvent::Char(' ');
    }

    // Printable characters
    for &(kc, normal, shifted) in &PRINTABLE {
        if kc == keycode {
            let ch = if shift { shifted } else { normal };
            if ctrl {
                // Ctrl+letter: always use lowercase letter
                let ctrl_ch = if ch.is_ascii_alphabetic() {
                    ch.to_ascii_lowercase()
                } else {
                    ch
                };
                return KeyEvent::Ctrl(ctrl_ch);
            }
            return KeyEvent::Char(ch);
        }
    }

    // Modifier-only keys (Shift, Control, etc.) — ignore
    if keycode == 37 || keycode == 50 || keycode == 62 || keycode == 64 {
        return KeyEvent::Unknown(keycode);
    }

    KeyEvent::Unknown(keycode)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_key() {
        assert_eq!(keycode_to_event(9, 0), KeyEvent::Escape);
    }

    #[test]
    fn enter_key() {
        assert_eq!(keycode_to_event(36, 0), KeyEvent::Enter);
    }

    #[test]
    fn backspace_key() {
        assert_eq!(keycode_to_event(22, 0), KeyEvent::Backspace);
    }

    #[test]
    fn tab_key() {
        assert_eq!(keycode_to_event(23, 0), KeyEvent::Tab);
    }

    #[test]
    fn delete_key() {
        assert_eq!(keycode_to_event(119, 0), KeyEvent::Delete);
    }

    #[test]
    fn arrow_keys() {
        assert_eq!(keycode_to_event(111, 0), KeyEvent::Up);
        assert_eq!(keycode_to_event(116, 0), KeyEvent::Down);
        assert_eq!(keycode_to_event(113, 0), KeyEvent::Left);
        assert_eq!(keycode_to_event(114, 0), KeyEvent::Right);
    }

    #[test]
    fn home_end_page() {
        assert_eq!(keycode_to_event(110, 0), KeyEvent::Home);
        assert_eq!(keycode_to_event(115, 0), KeyEvent::End);
        assert_eq!(keycode_to_event(112, 0), KeyEvent::PageUp);
        assert_eq!(keycode_to_event(117, 0), KeyEvent::PageDown);
    }

    #[test]
    fn function_keys() {
        assert_eq!(keycode_to_event(67, 0), KeyEvent::F(1));
        assert_eq!(keycode_to_event(76, 0), KeyEvent::F(10));
        assert_eq!(keycode_to_event(95, 0), KeyEvent::F(11));
        assert_eq!(keycode_to_event(96, 0), KeyEvent::F(12));
    }

    #[test]
    fn letter_keys_normal() {
        assert_eq!(keycode_to_event(38, 0), KeyEvent::Char('a'));
        assert_eq!(keycode_to_event(52, 0), KeyEvent::Char('z'));
        assert_eq!(keycode_to_event(24, 0), KeyEvent::Char('q'));
    }

    #[test]
    fn letter_keys_shifted() {
        let shift = SHIFT_MASK;
        assert_eq!(keycode_to_event(38, shift), KeyEvent::Char('A'));
        assert_eq!(keycode_to_event(52, shift), KeyEvent::Char('Z'));
    }

    #[test]
    fn number_keys_normal() {
        assert_eq!(keycode_to_event(10, 0), KeyEvent::Char('1'));
        assert_eq!(keycode_to_event(19, 0), KeyEvent::Char('0'));
    }

    #[test]
    fn number_keys_shifted() {
        let shift = SHIFT_MASK;
        assert_eq!(keycode_to_event(10, shift), KeyEvent::Char('!'));
        assert_eq!(keycode_to_event(11, shift), KeyEvent::Char('@'));
        assert_eq!(keycode_to_event(12, shift), KeyEvent::Char('#'));
    }

    #[test]
    fn symbol_keys() {
        assert_eq!(keycode_to_event(20, 0), KeyEvent::Char('-'));
        assert_eq!(keycode_to_event(20, SHIFT_MASK), KeyEvent::Char('_'));
        assert_eq!(keycode_to_event(47, 0), KeyEvent::Char(';'));
        assert_eq!(keycode_to_event(47, SHIFT_MASK), KeyEvent::Char(':'));
        assert_eq!(keycode_to_event(34, 0), KeyEvent::Char('['));
        assert_eq!(keycode_to_event(34, SHIFT_MASK), KeyEvent::Char('{'));
    }

    #[test]
    fn space_key() {
        assert_eq!(keycode_to_event(65, 0), KeyEvent::Char(' '));
    }

    #[test]
    fn ctrl_combinations() {
        let ctrl = CONTROL_MASK;
        assert_eq!(keycode_to_event(38, ctrl), KeyEvent::Ctrl('a'));
        assert_eq!(keycode_to_event(54, ctrl), KeyEvent::Ctrl('c'));
        assert_eq!(keycode_to_event(55, ctrl), KeyEvent::Ctrl('v'));
        assert_eq!(keycode_to_event(46, ctrl), KeyEvent::Ctrl('l'));
        assert_eq!(keycode_to_event(28, ctrl), KeyEvent::Ctrl('t'));
        assert_eq!(keycode_to_event(25, ctrl), KeyEvent::Ctrl('w'));
    }

    #[test]
    fn ctrl_shift_uses_lowercase() {
        let ctrl_shift = CONTROL_MASK | SHIFT_MASK;
        assert_eq!(keycode_to_event(38, ctrl_shift), KeyEvent::Ctrl('a'));
    }

    #[test]
    fn unknown_keycode() {
        // Modifier-only keys or unmapped keycodes
        assert_eq!(keycode_to_event(37, 0), KeyEvent::Unknown(37)); // Control_L
        assert_eq!(keycode_to_event(50, 0), KeyEvent::Unknown(50)); // Shift_L
        assert_eq!(keycode_to_event(200, 0), KeyEvent::Unknown(200));
    }
}
