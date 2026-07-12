// ---- Password hashing (RWC Ch8/Ch16): PBKDF2-HMAC-SHA256 + Argon2id ----
// Pure Rust embedded runtime — RFC 8018 / RFC 9106. Validated against RFC 6070
// and RustCrypto argon2 KATs (m=32,t=3,p=1 and p=4). Do NOT use raw SHA as a
// password hash (RWC: fast hashes + offline attack = cracked stores).

/// RFC 8018 PBKDF2-HMAC-SHA256.
fn anubis_pbkdf2_hmac_sha256_raw(
    password: &[u8],
    salt: &[u8],
    iterations: u32,
    dk_len: usize,
) -> Vec<u8> {
    if iterations < 1 {
        panic!("ANUBIS_CRYPTO_PBKDF2_ITERATIONS: must be >= 1");
    }
    if dk_len > 1024 * 1024 {
        panic!("ANUBIS_CRYPTO_PBKDF2_TOO_LONG: max 1MiB");
    }
    if salt.is_empty() {
        panic!("ANUBIS_CRYPTO_PBKDF2_SALT: salt must be non-empty (prefer >= 16 bytes)");
    }
    let mut out = Vec::with_capacity(dk_len);
    let mut block_index: u32 = 1;
    while out.len() < dk_len {
        let mut block = salt.to_vec();
        block.extend_from_slice(&block_index.to_be_bytes());
        let mut u = anubis_hmac_sha256_raw(password, &block);
        let mut t = u;
        for _ in 1..iterations {
            u = anubis_hmac_sha256_raw(password, &u);
            for i in 0..32 {
                t[i] ^= u[i];
            }
        }
        out.extend_from_slice(&t);
        block_index = block_index.wrapping_add(1);
        if block_index == 0 {
            panic!("ANUBIS_CRYPTO_PBKDF2_OVERFLOW");
        }
    }
    out.truncate(dk_len);
    out
}

fn anubis_pbkdf2_hmac_sha256(
    password: AnubisValue,
    salt: AnubisValue,
    iterations: AnubisValue,
    length: AnubisValue,
) -> AnubisValue {
    let iters = iterations.as_i64();
    if iters < 1 || iters > u32::MAX as i64 {
        panic!("ANUBIS_CRYPTO_PBKDF2_ITERATIONS: must be in 1..2^32-1");
    }
    let n = length.as_i64().max(0) as usize;
    let dk = anubis_pbkdf2_hmac_sha256_raw(
        &anubis_crypto_bytes(&password),
        &anubis_crypto_bytes(&salt),
        iters as u32,
        n,
    );
    anubis_bytes_list(&dk)
}

// ---- Blake2b (for Argon2 H / H') ----
const ANUBIS_BLAKE2B_IV: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];
const ANUBIS_BLAKE2B_SIGMA: [[usize; 16]; 12] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
];

fn anubis_blake2b_g(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
    v[d] = (v[d] ^ v[a]).rotate_right(32);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(24);
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
    v[d] = (v[d] ^ v[a]).rotate_right(16);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(63);
}

fn anubis_blake2b_compress(h: &mut [u64; 8], block: &[u8; 128], t: u64, last: bool) {
    let mut m = [0u64; 16];
    for i in 0..16 {
        let mut b = [0u8; 8];
        b.copy_from_slice(&block[i * 8..i * 8 + 8]);
        m[i] = u64::from_le_bytes(b);
    }
    let mut v = [0u64; 16];
    v[..8].copy_from_slice(h);
    v[8..16].copy_from_slice(&ANUBIS_BLAKE2B_IV);
    v[12] ^= t;
    if last {
        v[14] = !v[14];
    }
    for r in 0..12 {
        let s = &ANUBIS_BLAKE2B_SIGMA[r];
        anubis_blake2b_g(&mut v, 0, 4, 8, 12, m[s[0]], m[s[1]]);
        anubis_blake2b_g(&mut v, 1, 5, 9, 13, m[s[2]], m[s[3]]);
        anubis_blake2b_g(&mut v, 2, 6, 10, 14, m[s[4]], m[s[5]]);
        anubis_blake2b_g(&mut v, 3, 7, 11, 15, m[s[6]], m[s[7]]);
        anubis_blake2b_g(&mut v, 0, 5, 10, 15, m[s[8]], m[s[9]]);
        anubis_blake2b_g(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
        anubis_blake2b_g(&mut v, 2, 7, 8, 13, m[s[12]], m[s[13]]);
        anubis_blake2b_g(&mut v, 3, 4, 9, 14, m[s[14]], m[s[15]]);
    }
    for i in 0..8 {
        h[i] ^= v[i] ^ v[i + 8];
    }
}

fn anubis_blake2b(data: &[u8], out_len: usize) -> Vec<u8> {
    if out_len < 1 || out_len > 64 {
        panic!("ANUBIS_CRYPTO_BLAKE2B_OUTLEN: must be 1..64");
    }
    let mut h = ANUBIS_BLAKE2B_IV;
    h[0] ^= 0x01010000 ^ (out_len as u64);
    let mut t: u64 = 0;
    let mut buf = [0u8; 128];
    let mut buflen = 0usize;
    let mut offset = 0usize;
    while offset < data.len() {
        if buflen == 128 {
            t += 128;
            anubis_blake2b_compress(&mut h, &buf, t, false);
            buflen = 0;
        }
        let take = (128 - buflen).min(data.len() - offset);
        buf[buflen..buflen + take].copy_from_slice(&data[offset..offset + take]);
        buflen += take;
        offset += take;
    }
    t += buflen as u64;
    for i in buflen..128 {
        buf[i] = 0;
    }
    anubis_blake2b_compress(&mut h, &buf, t, true);
    let mut full = [0u8; 64];
    for i in 0..8 {
        full[i * 8..i * 8 + 8].copy_from_slice(&h[i].to_le_bytes());
    }
    full[..out_len].to_vec()
}

fn anubis_blake2b_long(inputs: &[&[u8]], out: &mut [u8]) {
    if out.is_empty() {
        panic!("ANUBIS_CRYPTO_BLAKE2B_LONG: empty output");
    }
    let len_bytes = (out.len() as u32).to_le_bytes();
    if out.len() <= 64 {
        let mut data = len_bytes.to_vec();
        for i in inputs {
            data.extend_from_slice(i);
        }
        let h = anubis_blake2b(&data, out.len());
        out.copy_from_slice(&h);
        return;
    }
    let half = 32;
    let mut data = len_bytes.to_vec();
    for i in inputs {
        data.extend_from_slice(i);
    }
    let mut last = anubis_blake2b(&data, 64);
    out[..half].copy_from_slice(&last[..half]);
    let mut counter = half;
    while out.len() - counter > 64 {
        last = anubis_blake2b(&last, 64);
        out[counter..counter + half].copy_from_slice(&last[..half]);
        counter += half;
    }
    let last_size = out.len() - counter;
    let h = anubis_blake2b(&last, last_size);
    out[counter..].copy_from_slice(&h);
}

// ---- Argon2id (RFC 9106) ----
const ANUBIS_ARGON2_SYNC_POINTS: usize = 4;
const ANUBIS_ARGON2_ADDRESSES_IN_BLOCK: usize = 128;
const ANUBIS_ARGON2_BLOCK_SIZE: usize = 1024;

#[derive(Clone, Copy)]
struct AnubisArgon2Block([u64; 128]);

impl AnubisArgon2Block {
    fn zero() -> Self {
        AnubisArgon2Block([0u64; 128])
    }
    fn load(&mut self, input: &[u8; ANUBIS_ARGON2_BLOCK_SIZE]) {
        for i in 0..128 {
            let mut b = [0u8; 8];
            b.copy_from_slice(&input[i * 8..i * 8 + 8]);
            self.0[i] = u64::from_le_bytes(b);
        }
    }
}

fn anubis_argon2_block_xor(a: &AnubisArgon2Block, b: &AnubisArgon2Block) -> AnubisArgon2Block {
    let mut o = AnubisArgon2Block::zero();
    for i in 0..128 {
        o.0[i] = a.0[i] ^ b.0[i];
    }
    o
}
fn anubis_argon2_block_xor_assign(a: &mut AnubisArgon2Block, b: &AnubisArgon2Block) {
    for i in 0..128 {
        a.0[i] ^= b.0[i];
    }
}

fn anubis_argon2_compress(rhs: &AnubisArgon2Block, lhs: &AnubisArgon2Block) -> AnubisArgon2Block {
    let r = anubis_argon2_block_xor(rhs, lhs);
    let mut q = r;
    for chunk_i in 0..8 {
        let base = chunk_i * 16;
        let mut v = [0u64; 16];
        v.copy_from_slice(&q.0[base..base + 16]);
        macro_rules! ps {
            ($a:expr, $b:expr, $c:expr, $d:expr) => {{
                const TRUNC: u64 = u32::MAX as u64;
                $a = $a
                    .wrapping_add($b)
                    .wrapping_add(2u64.wrapping_mul(($a & TRUNC).wrapping_mul($b & TRUNC)));
                $d = ($d ^ $a).rotate_right(32);
                $c = $c
                    .wrapping_add($d)
                    .wrapping_add(2u64.wrapping_mul(($c & TRUNC).wrapping_mul($d & TRUNC)));
                $b = ($b ^ $c).rotate_right(24);
                $a = $a
                    .wrapping_add($b)
                    .wrapping_add(2u64.wrapping_mul(($a & TRUNC).wrapping_mul($b & TRUNC)));
                $d = ($d ^ $a).rotate_right(16);
                $c = $c
                    .wrapping_add($d)
                    .wrapping_add(2u64.wrapping_mul(($c & TRUNC).wrapping_mul($d & TRUNC)));
                $b = ($b ^ $c).rotate_right(63);
            }};
        }
        ps!(v[0], v[4], v[8], v[12]);
        ps!(v[1], v[5], v[9], v[13]);
        ps!(v[2], v[6], v[10], v[14]);
        ps!(v[3], v[7], v[11], v[15]);
        ps!(v[0], v[5], v[10], v[15]);
        ps!(v[1], v[6], v[11], v[12]);
        ps!(v[2], v[7], v[8], v[13]);
        ps!(v[3], v[4], v[9], v[14]);
        q.0[base..base + 16].copy_from_slice(&v);
    }
    for i in 0..8 {
        let b = i * 2;
        let mut v = [
            q.0[b],
            q.0[b + 1],
            q.0[b + 16],
            q.0[b + 17],
            q.0[b + 32],
            q.0[b + 33],
            q.0[b + 48],
            q.0[b + 49],
            q.0[b + 64],
            q.0[b + 65],
            q.0[b + 80],
            q.0[b + 81],
            q.0[b + 96],
            q.0[b + 97],
            q.0[b + 112],
            q.0[b + 113],
        ];
        macro_rules! ps {
            ($a:expr, $b:expr, $c:expr, $d:expr) => {{
                const TRUNC: u64 = u32::MAX as u64;
                $a = $a
                    .wrapping_add($b)
                    .wrapping_add(2u64.wrapping_mul(($a & TRUNC).wrapping_mul($b & TRUNC)));
                $d = ($d ^ $a).rotate_right(32);
                $c = $c
                    .wrapping_add($d)
                    .wrapping_add(2u64.wrapping_mul(($c & TRUNC).wrapping_mul($d & TRUNC)));
                $b = ($b ^ $c).rotate_right(24);
                $a = $a
                    .wrapping_add($b)
                    .wrapping_add(2u64.wrapping_mul(($a & TRUNC).wrapping_mul($b & TRUNC)));
                $d = ($d ^ $a).rotate_right(16);
                $c = $c
                    .wrapping_add($d)
                    .wrapping_add(2u64.wrapping_mul(($c & TRUNC).wrapping_mul($d & TRUNC)));
                $b = ($b ^ $c).rotate_right(63);
            }};
        }
        ps!(v[0], v[4], v[8], v[12]);
        ps!(v[1], v[5], v[9], v[13]);
        ps!(v[2], v[6], v[10], v[14]);
        ps!(v[3], v[7], v[11], v[15]);
        ps!(v[0], v[5], v[10], v[15]);
        ps!(v[1], v[6], v[11], v[12]);
        ps!(v[2], v[7], v[8], v[13]);
        ps!(v[3], v[4], v[9], v[14]);
        q.0[b] = v[0];
        q.0[b + 1] = v[1];
        q.0[b + 16] = v[2];
        q.0[b + 17] = v[3];
        q.0[b + 32] = v[4];
        q.0[b + 33] = v[5];
        q.0[b + 48] = v[6];
        q.0[b + 49] = v[7];
        q.0[b + 64] = v[8];
        q.0[b + 65] = v[9];
        q.0[b + 80] = v[10];
        q.0[b + 81] = v[11];
        q.0[b + 96] = v[12];
        q.0[b + 97] = v[13];
        q.0[b + 112] = v[14];
        q.0[b + 113] = v[15];
    }
    anubis_argon2_block_xor(&q, &r)
}

/// Argon2id raw (RFC 9106). m_cost is KiB of memory. Prefer m>=19456, t>=2, p=1 (OWASP).
fn anubis_argon2id_raw(
    pwd: &[u8],
    salt: &[u8],
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
    out_len: usize,
) -> Vec<u8> {
    if salt.len() < 8 {
        panic!("ANUBIS_CRYPTO_ARGON2_SALT: salt must be >= 8 bytes (prefer 16)");
    }
    if p_cost < 1 || t_cost < 1 {
        panic!("ANUBIS_CRYPTO_ARGON2_PARAMS: t and p must be >= 1");
    }
    if out_len < 4 || out_len > 1024 {
        panic!("ANUBIS_CRYPTO_ARGON2_OUTLEN: must be 4..1024");
    }
    // Cap memory for safety in educational / embedded run (max 256 MiB)
    if m_cost < 8 || m_cost > 256 * 1024 {
        panic!("ANUBIS_CRYPTO_ARGON2_M: m_kib must be in 8..262144");
    }
    let lanes = p_cost as usize;
    let m_cost_usz = m_cost as usize;
    let memory_blocks = if m_cost_usz < 2 * ANUBIS_ARGON2_SYNC_POINTS * lanes {
        2 * ANUBIS_ARGON2_SYNC_POINTS * lanes
    } else {
        m_cost_usz
    };
    let segment_length = memory_blocks / (lanes * ANUBIS_ARGON2_SYNC_POINTS);
    let lane_length = segment_length * ANUBIS_ARGON2_SYNC_POINTS;
    let block_count = segment_length * lanes * ANUBIS_ARGON2_SYNC_POINTS;
    let algorithm: u32 = 2; // Argon2id
    let version: u32 = 0x13;

    let mut h0_in = Vec::new();
    h0_in.extend_from_slice(&p_cost.to_le_bytes());
    h0_in.extend_from_slice(&(out_len as u32).to_le_bytes());
    h0_in.extend_from_slice(&m_cost.to_le_bytes());
    h0_in.extend_from_slice(&t_cost.to_le_bytes());
    h0_in.extend_from_slice(&version.to_le_bytes());
    h0_in.extend_from_slice(&algorithm.to_le_bytes());
    h0_in.extend_from_slice(&(pwd.len() as u32).to_le_bytes());
    h0_in.extend_from_slice(pwd);
    h0_in.extend_from_slice(&(salt.len() as u32).to_le_bytes());
    h0_in.extend_from_slice(salt);
    h0_in.extend_from_slice(&0u32.to_le_bytes());
    h0_in.extend_from_slice(&0u32.to_le_bytes());
    let initial_hash = anubis_blake2b(&h0_in, 64);

    let mut memory = vec![AnubisArgon2Block::zero(); block_count];
    for l in 0..lanes {
        for i in 0..2u32 {
            let l_bytes = (l as u32).to_le_bytes();
            let i_bytes = i.to_le_bytes();
            let inputs: [&[u8]; 3] = [&initial_hash, &i_bytes, &l_bytes];
            let mut hash = [0u8; ANUBIS_ARGON2_BLOCK_SIZE];
            anubis_blake2b_long(&inputs, &mut hash);
            memory[l * lane_length + i as usize].load(&hash);
        }
    }

    let iterations = t_cost as usize;
    for pass in 0..iterations {
        for slice in 0..ANUBIS_ARGON2_SYNC_POINTS {
            let data_independent = pass == 0 && slice < ANUBIS_ARGON2_SYNC_POINTS / 2;
            for lane in 0..lanes {
                let mut address_block = AnubisArgon2Block::zero();
                let mut input_block = AnubisArgon2Block::zero();
                let zero_block = AnubisArgon2Block::zero();
                if data_independent {
                    input_block.0[0] = pass as u64;
                    input_block.0[1] = lane as u64;
                    input_block.0[2] = slice as u64;
                    input_block.0[3] = memory.len() as u64;
                    input_block.0[4] = iterations as u64;
                    input_block.0[5] = algorithm as u64;
                }
                let first_block = if pass == 0 && slice == 0 {
                    if data_independent {
                        input_block.0[6] += 1;
                        address_block = anubis_argon2_compress(&zero_block, &input_block);
                        address_block = anubis_argon2_compress(&zero_block, &address_block);
                    }
                    2
                } else {
                    0
                };
                let mut cur_index = lane * lane_length + slice * segment_length + first_block;
                let mut prev_index = if slice == 0 && first_block == 0 {
                    cur_index + lane_length - 1
                } else {
                    cur_index - 1
                };
                for block in first_block..segment_length {
                    let rand = if data_independent {
                        let addr_index = block % ANUBIS_ARGON2_ADDRESSES_IN_BLOCK;
                        if addr_index == 0 {
                            input_block.0[6] += 1;
                            address_block = anubis_argon2_compress(&zero_block, &input_block);
                            address_block = anubis_argon2_compress(&zero_block, &address_block);
                        }
                        address_block.0[addr_index]
                    } else {
                        memory[prev_index].0[0]
                    };
                    let ref_lane = if pass == 0 && slice == 0 {
                        lane
                    } else {
                        ((rand >> 32) as usize) % lanes
                    };
                    let reference_area_size = if pass == 0 {
                        if slice == 0 {
                            block - 1
                        } else if ref_lane == lane {
                            slice * segment_length + block - 1
                        } else {
                            slice * segment_length - if block == 0 { 1 } else { 0 }
                        }
                    } else if ref_lane == lane {
                        lane_length - segment_length + block - 1
                    } else {
                        lane_length - segment_length - if block == 0 { 1 } else { 0 }
                    };
                    let mut map = rand & 0xFFFFFFFF;
                    map = (map * map) >> 32;
                    let relative_position = reference_area_size
                        - 1
                        - ((reference_area_size as u64 * map) >> 32) as usize;
                    let start_position = if pass != 0 && slice != ANUBIS_ARGON2_SYNC_POINTS - 1 {
                        (slice + 1) * segment_length
                    } else {
                        0
                    };
                    let lane_index = (start_position + relative_position) % lane_length;
                    let ref_index = ref_lane * lane_length + lane_index;
                    let result = anubis_argon2_compress(&memory[prev_index], &memory[ref_index]);
                    if pass == 0 {
                        memory[cur_index] = result;
                    } else {
                        anubis_argon2_block_xor_assign(&mut memory[cur_index], &result);
                    }
                    prev_index = cur_index;
                    cur_index += 1;
                }
            }
        }
    }

    let mut blockhash = memory[lane_length - 1];
    for l in 1..lanes {
        let last = l * lane_length + (lane_length - 1);
        anubis_argon2_block_xor_assign(&mut blockhash, &memory[last]);
    }
    let mut blockhash_bytes = [0u8; ANUBIS_ARGON2_BLOCK_SIZE];
    for i in 0..128 {
        blockhash_bytes[i * 8..i * 8 + 8].copy_from_slice(&blockhash.0[i].to_le_bytes());
    }
    let mut out = vec![0u8; out_len];
    anubis_blake2b_long(&[&blockhash_bytes], &mut out);
    out
}

fn anubis_argon2id_hash(
    password: AnubisValue,
    salt: AnubisValue,
    m_kib: AnubisValue,
    t: AnubisValue,
    p: AnubisValue,
    out_len: AnubisValue,
) -> AnubisValue {
    let m = m_kib.as_i64();
    let tt = t.as_i64();
    let pp = p.as_i64();
    let ol = out_len.as_i64();
    if m < 8 || m > 256 * 1024 {
        panic!("ANUBIS_CRYPTO_ARGON2_M: m_kib must be in 8..262144");
    }
    if tt < 1 || tt > 100 {
        panic!("ANUBIS_CRYPTO_ARGON2_T: time cost must be in 1..100");
    }
    if pp < 1 || pp > 16 {
        panic!("ANUBIS_CRYPTO_ARGON2_P: parallelism must be in 1..16");
    }
    if ol < 4 || ol > 1024 {
        panic!("ANUBIS_CRYPTO_ARGON2_OUTLEN: must be 4..1024");
    }
    let hash = anubis_argon2id_raw(
        &anubis_crypto_bytes(&password),
        &anubis_crypto_bytes(&salt),
        m as u32,
        tt as u32,
        pp as u32,
        ol as usize,
    );
    anubis_bytes_list(&hash)
}

fn anubis_hex_decode_loose(s: &str) -> Option<Vec<u8>> {
    let chars: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() % 2 != 0 {
        return None;
    }
    if !chars.iter().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = Vec::with_capacity(chars.len() / 2);
    let mut i = 0;
    while i < chars.len() {
        let byte = u8::from_str_radix(&format!("{}{}", chars[i], chars[i + 1]), 16).ok()?;
        out.push(byte);
        i += 2;
    }
    Some(out)
}

/// Production password hash: Argon2id m=19456 (19 MiB), t=2, p=1, 16-byte salt, 32-byte tag.
/// Encoding: `anubis$argon2id$v=19$m=19456,t=2,p=1$<hexsalt>$<hexhash>`
fn anubis_password_hash_encode(password: AnubisValue) -> AnubisValue {
    let salt_v = anubis_random_bytes(AnubisValue::Int(16));
    let salt = anubis_crypto_bytes(&salt_v);
    let hash = anubis_argon2id_raw(&anubis_crypto_bytes(&password), &salt, 19456, 2, 1, 32);
    let enc = format!(
        "anubis$argon2id$v=19$m=19456,t=2,p=1${}${}",
        anubis_hex_encode(&salt),
        anubis_hex_encode(&hash)
    );
    anubis_mk_str(enc)
}

/// Constant-time password verify against `anubis$argon2id$...` or `anubis$pbkdf2-sha256$...`.
fn anubis_password_verify_encoding(password: AnubisValue, encoding: AnubisValue) -> AnubisValue {
    // Encodings (must match audited host path):
    //   anubis$argon2id$v=19$m=M,t=T,p=P$hexsalt$hexhash   (6 fields)
    //   anubis$pbkdf2-sha256$i=ITERS$hexsalt$hexhash         (5 fields)
    let enc = encoding.display_string();
    let parts: Vec<&str> = enc.split('$').collect();
    if parts.len() < 5 || parts[0] != "anubis" {
        return AnubisValue::Bool(false);
    }
    let algo = parts[1];
    let pwd = anubis_crypto_bytes(&password);
    let salt = match anubis_hex_decode_loose(parts[parts.len() - 2]) {
        Some(s) if !s.is_empty() => s,
        _ => return AnubisValue::Bool(false),
    };
    let expected = match anubis_hex_decode_loose(parts[parts.len() - 1]) {
        Some(h) if !h.is_empty() => h,
        _ => return AnubisValue::Bool(false),
    };
    let got = if algo == "argon2id" {
        if parts.len() < 6 {
            return AnubisValue::Bool(false);
        }
        let mut m: Option<u32> = None;
        let mut t: Option<u32> = None;
        let mut p: Option<u32> = None;
        for kv in parts[3].split(',') {
            if let Some(v) = kv.strip_prefix("m=") {
                m = v.parse().ok();
            } else if let Some(v) = kv.strip_prefix("t=") {
                t = v.parse().ok();
            } else if let Some(v) = kv.strip_prefix("p=") {
                p = v.parse().ok();
            }
        }
        let (Some(m), Some(t), Some(p)) = (m, t, p) else {
            return AnubisValue::Bool(false);
        };
        anubis_argon2id_raw(&pwd, &salt, m, t, p, expected.len())
    } else if algo == "pbkdf2-sha256" {
        let iters: u32 = match parts[2].strip_prefix("i=").and_then(|v| v.parse().ok()) {
            Some(i) if i >= 1 => i,
            _ => return AnubisValue::Bool(false),
        };
        anubis_pbkdf2_hmac_sha256_raw(&pwd, &salt, iters, expected.len())
    } else {
        return AnubisValue::Bool(false);
    };
    if got.len() != expected.len() {
        return AnubisValue::Bool(false);
    }
    let mut v = 0u8;
    for i in 0..got.len() {
        v |= got[i] ^ expected[i];
    }
    AnubisValue::Bool(v == 0)
}

/// Convenience: PBKDF2 password hash with OWASP-ish 600k iterations (fallback if Argon2 too heavy).
fn anubis_password_hash_pbkdf2_encode(password: AnubisValue) -> AnubisValue {
    let salt_v = anubis_random_bytes(AnubisValue::Int(16));
    let salt = anubis_crypto_bytes(&salt_v);
    let hash = anubis_pbkdf2_hmac_sha256_raw(&anubis_crypto_bytes(&password), &salt, 600_000, 32);
    let enc = format!(
        "anubis$pbkdf2-sha256$i=600000${}${}",
        anubis_hex_encode(&salt),
        anubis_hex_encode(&hash)
    );
    anubis_mk_str(enc)
}
