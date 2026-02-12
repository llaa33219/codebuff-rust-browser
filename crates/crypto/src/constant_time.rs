/// Constant-time comparison utilities for cryptographic operations.
///
/// These functions avoid branching on secret data to prevent timing side-channel attacks.

/// Constant-time equality comparison of two byte slices.
///
/// Returns `true` if and only if `a` and `b` have the same length and identical contents.
/// The comparison always examines every byte to avoid leaking information via timing.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut x: u8 = 0;
    for i in 0..a.len() {
        x |= a[i] ^ b[i];
    }
    x == 0
}

/// Constant-time select: returns `a` if `choice == 0`, or `b` if `choice == 1`.
///
/// `choice` MUST be 0 or 1. Behavior is undefined for other values.
pub fn ct_select_u8(a: u8, b: u8, choice: u8) -> u8 {
    // mask is 0x00 if choice==0, 0xFF if choice==1
    let mask = (0u8).wrapping_sub(choice);
    a ^ (mask & (a ^ b))
}

/// Constant-time conditional copy: if `choice == 1`, copies `src` into `dst`.
/// If `choice == 0`, `dst` is unchanged.
///
/// `choice` MUST be 0 or 1.
pub fn ct_copy_if(dst: &mut [u8], src: &[u8], choice: u8) {
    assert_eq!(dst.len(), src.len());
    let mask = (0u8).wrapping_sub(choice);
    for i in 0..dst.len() {
        dst[i] ^= mask & (dst[i] ^ src[i]);
    }
}

/// Constant-time check if a byte slice is all zeros.
pub fn ct_is_zero(data: &[u8]) -> bool {
    let mut acc: u8 = 0;
    for &b in data {
        acc |= b;
    }
    acc == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ct_eq_equal() {
        let a = [1u8, 2, 3, 4, 5];
        let b = [1u8, 2, 3, 4, 5];
        assert!(ct_eq(&a, &b));
    }

    #[test]
    fn test_ct_eq_not_equal() {
        let a = [1u8, 2, 3, 4, 5];
        let b = [1u8, 2, 3, 4, 6];
        assert!(!ct_eq(&a, &b));
    }

    #[test]
    fn test_ct_eq_different_lengths() {
        let a = [1u8, 2, 3];
        let b = [1u8, 2, 3, 4];
        assert!(!ct_eq(&a, &b));
    }

    #[test]
    fn test_ct_eq_empty() {
        let a: [u8; 0] = [];
        let b: [u8; 0] = [];
        assert!(ct_eq(&a, &b));
    }

    #[test]
    fn test_ct_select_u8() {
        assert_eq!(ct_select_u8(0xAA, 0xBB, 0), 0xAA);
        assert_eq!(ct_select_u8(0xAA, 0xBB, 1), 0xBB);
    }

    #[test]
    fn test_ct_copy_if() {
        let mut dst = [1u8, 2, 3];
        let src = [4u8, 5, 6];
        ct_copy_if(&mut dst, &src, 0);
        assert_eq!(dst, [1, 2, 3]);
        ct_copy_if(&mut dst, &src, 1);
        assert_eq!(dst, [4, 5, 6]);
    }

    #[test]
    fn test_ct_is_zero() {
        assert!(ct_is_zero(&[0, 0, 0, 0]));
        assert!(!ct_is_zero(&[0, 0, 1, 0]));
        assert!(ct_is_zero(&[]));
    }
}
