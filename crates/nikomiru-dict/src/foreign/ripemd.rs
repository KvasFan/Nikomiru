// SPDX-License-Identifier: GPL-3.0-only

//! RIPEMD-128, which MDict uses to make the key of its index (DIC-004): an
//! obscuring, not a protection, so nothing here is for secrets.

const R: [[usize; 16]; 8] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [7, 4, 13, 1, 10, 6, 15, 3, 12, 0, 9, 5, 2, 14, 11, 8],
    [3, 10, 14, 4, 9, 15, 8, 1, 2, 7, 0, 6, 13, 11, 5, 12],
    [1, 9, 11, 10, 0, 8, 12, 4, 13, 3, 7, 15, 14, 5, 6, 2],
    [5, 14, 7, 0, 9, 2, 11, 4, 13, 6, 15, 8, 1, 10, 3, 12],
    [6, 11, 3, 7, 0, 13, 5, 10, 14, 15, 8, 12, 4, 9, 1, 2],
    [15, 5, 1, 3, 7, 14, 6, 9, 11, 8, 12, 2, 10, 0, 4, 13],
    [8, 6, 4, 1, 3, 11, 15, 0, 5, 12, 2, 13, 9, 7, 10, 14],
];
const S: [[u32; 16]; 8] = [
    [11, 14, 15, 12, 5, 8, 7, 9, 11, 13, 14, 15, 6, 7, 9, 8],
    [7, 6, 8, 13, 11, 9, 7, 15, 7, 12, 15, 9, 11, 7, 13, 12],
    [11, 13, 6, 7, 14, 9, 13, 15, 14, 8, 13, 6, 5, 12, 7, 5],
    [11, 12, 14, 15, 14, 15, 9, 8, 9, 14, 5, 6, 8, 6, 5, 12],
    [8, 9, 9, 11, 13, 15, 15, 5, 7, 7, 8, 11, 14, 14, 12, 6],
    [9, 13, 15, 7, 12, 8, 9, 11, 7, 7, 12, 7, 6, 15, 13, 11],
    [9, 7, 15, 11, 8, 6, 6, 14, 12, 13, 5, 14, 13, 13, 7, 5],
    [15, 5, 8, 11, 14, 14, 6, 14, 6, 9, 12, 9, 12, 5, 15, 8],
];
const K: [u32; 8] = [
    0,
    0x5a82_7999,
    0x6ed9_eba1,
    0x8f1b_bcdc,
    0x50a2_8be6,
    0x5c4d_d124,
    0x6d70_3ef3,
    0,
];

fn f(round: usize, x: u32, y: u32, z: u32) -> u32 {
    match round {
        0 => x ^ y ^ z,
        1 => (x & y) | (!x & z),
        2 => (x | !y) ^ z,
        _ => (x & z) | (y & !z),
    }
}

fn block(state: &mut [u32; 4], words: &[u32; 16]) {
    let run = |left: bool| {
        let [mut a, mut b, mut c, mut d] = *state;
        for round in 0..4 {
            let set = if left { round } else { 4 + round };
            let function = if left { round } else { 3 - round };
            for j in 0..16 {
                let t = a
                    .wrapping_add(f(function, b, c, d))
                    .wrapping_add(words[R[set][j]])
                    .wrapping_add(K[set])
                    .rotate_left(S[set][j]);
                (a, d, c, b) = (d, c, b, t);
            }
        }
        [a, b, c, d]
    };
    let [a, b, c, d] = run(true);
    let [a2, b2, c2, d2] = run(false);
    let t = state[1].wrapping_add(c).wrapping_add(d2);
    state[1] = state[2].wrapping_add(d).wrapping_add(a2);
    state[2] = state[3].wrapping_add(a).wrapping_add(b2);
    state[3] = state[0].wrapping_add(b).wrapping_add(c2);
    state[0] = t;
}

/// The digest of `data`.
pub(super) fn digest(data: &[u8]) -> [u8; 16] {
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_le_bytes());
    let mut state = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    let (blocks, _) = message.as_chunks::<64>();
    for chunk in blocks {
        let mut words = [0u32; 16];
        for (word, bytes) in words.iter_mut().zip(chunk.as_chunks::<4>().0) {
            *word = u32::from_le_bytes(*bytes);
        }
        block(&mut state, &words);
    }
    let mut out = [0u8; 16];
    for (bytes, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(state) {
        *bytes = word.to_le_bytes();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    // T-DIC-004: the vectors of the RIPEMD-128 specification
    #[test]
    fn the_published_vectors() {
        assert_eq!(hex(&digest(b"")), "cdf26213a150dc3ecb610f18f6b38b46");
        assert_eq!(hex(&digest(b"a")), "86be7afa339d0fc7cfc785e72f578d33");
        assert_eq!(hex(&digest(b"abc")), "c14a12199c66e4ba84636b0f69144c77");
        assert_eq!(
            hex(&digest(b"abcdefghijklmnopqrstuvwxyz")),
            "fd2aa607f71dc8f510714922b371834e"
        );
    }
}
