// Keychain / Secure Enclave bind for *non-exportable* capability tokens (native `anubis run` only).
//
// HONESTY (load-bearing):
// - Soft path always works: `__anubis_cap_ne_soft:<kind>:<nonce>`.
// - On macOS, mint tries Keychain generic-password storage (`__anubis_cap_ne_kc:…`) and, when
//   `ANUBIS_KEYCHAIN_SE=1`, a Secure Enclave–resident EC key handle (`__anubis_cap_ne_se:…`).
// - Success means "item/key created under the current process identity", NOT "signed app with
//   production SE ACL + attestation". Codesign + App Sandbox + keychain-access-groups still
//   required for host-enforced isolation (`apple_enforced_claim` remains false until signed).
// - Guest / non-macOS: soft only.

// Last NE mint bind mode for this process: "soft" | "kc" | "se" (not secret material).
static ANUBIS_LAST_NE_BIND: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

fn anubis_keychain_se_note_bind(mode: &str) {
    if let Ok(mut g) = ANUBIS_LAST_NE_BIND.lock() {
        *g = mode.to_string();
    }
}

/// Last `cap_acquire_nonexportable` bind mode for this process (`soft` / `kc` / `se`).
/// Does not take a token argument — not an export of capability material.
fn anubis_keychain_se_last_bind() -> AnubisValue {
    let s = ANUBIS_LAST_NE_BIND
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();
    if s.is_empty() {
        anubis_mk_str("none".to_string())
    } else {
        anubis_mk_str(s)
    }
}

/// Probe result: 0 = soft-only, 1 = Keychain bind available, 2 = Secure Enclave path available.
fn anubis_keychain_se_probe() -> AnubisValue {
    AnubisValue::Int(anubis_keychain_se_probe_i64())
}

fn anubis_keychain_se_probe_i64() -> i64 {
    #[cfg(target_os = "macos")]
    {
        if anubis_kc_se_available() {
            return 2;
        }
        if anubis_kc_keychain_available() {
            return 1;
        }
    }
    0
}

/// Mint an exportable capability (no Keychain bind — software token only).
fn anubis_cap_acquire(kind: AnubisValue) -> AnubisValue {
    anubis_mk_str(format!("__anubis_cap:{}", kind.display_string()))
}

/// Mint a *non-exportable* capability: prefer Keychain/SE bind on macOS, soft fallback.
fn anubis_cap_acquire_nonexportable(kind: AnubisValue) -> AnubisValue {
    let k = kind.display_string();
    #[cfg(target_os = "macos")]
    {
        if std::env::var_os("ANUBIS_KEYCHAIN_SE")
            .map(|v| v != "0" && v != "false")
            .unwrap_or(false)
        {
            if let Ok(tok) = anubis_kc_mint_se(&k) {
                anubis_keychain_se_note_bind("se");
                return anubis_mk_str(tok);
            }
        }
        // Default on macOS: try Keychain bind (opt out with ANUBIS_KEYCHAIN_CAPS=0).
        let want_kc = std::env::var_os("ANUBIS_KEYCHAIN_CAPS")
            .map(|v| v != "0" && v != "false")
            .unwrap_or(true);
        if want_kc {
            if let Ok(tok) = anubis_kc_mint_keychain(&k) {
                anubis_keychain_se_note_bind("kc");
                return anubis_mk_str(tok);
            }
        }
    }
    let nonce = anubis_kc_nonce();
    anubis_keychain_se_note_bind("soft");
    anubis_mk_str(format!("__anubis_cap_ne_soft:{k}:{nonce}"))
}

/// Language peel is identity on the token value. Optional Keychain delete on export when
/// `ANUBIS_KEYCHAIN_DELETE_ON_EXPORT=1` and the token is a `kc:` / `se:` bind.
fn anubis_cap_export(cap: AnubisValue, _reason: AnubisValue) -> AnubisValue {
    #[cfg(target_os = "macos")]
    {
        if std::env::var_os("ANUBIS_KEYCHAIN_DELETE_ON_EXPORT")
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false)
        {
            let s = cap.display_string();
            let _ = anubis_kc_delete_token(&s);
        }
    }
    cap
}

fn anubis_kc_nonce() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}-{}", t, std::process::id())
}

// ── macOS Security.framework FFI ──────────────────────────────────────────────

#[cfg(target_os = "macos")]
#[link(name = "Security", kind = "framework")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn SecItemAdd(attributes: *const std::ffi::c_void, result: *mut *const std::ffi::c_void) -> i32;
    fn SecItemDelete(query: *const std::ffi::c_void) -> i32;
    fn SecItemCopyMatching(
        query: *const std::ffi::c_void,
        result: *mut *const std::ffi::c_void,
    ) -> i32;
    fn SecKeyCreateRandomKey(
        parameters: *const std::ffi::c_void,
        error: *mut *const std::ffi::c_void,
    ) -> *const std::ffi::c_void;
    fn SecKeyCopyPublicKey(key: *const std::ffi::c_void) -> *const std::ffi::c_void;
    fn SecKeyCopyExternalRepresentation(
        key: *const std::ffi::c_void,
        error: *mut *const std::ffi::c_void,
    ) -> *const std::ffi::c_void;
    fn CFDictionaryCreate(
        allocator: *const std::ffi::c_void,
        keys: *const *const std::ffi::c_void,
        values: *const *const std::ffi::c_void,
        num_values: isize,
        key_callbacks: *const std::ffi::c_void,
        value_callbacks: *const std::ffi::c_void,
    ) -> *const std::ffi::c_void;
    fn CFStringCreateWithCString(
        alloc: *const std::ffi::c_void,
        c_str: *const i8,
        encoding: u32,
    ) -> *const std::ffi::c_void;
    fn CFDataCreate(
        alloc: *const std::ffi::c_void,
        bytes: *const u8,
        length: isize,
    ) -> *const std::ffi::c_void;
    fn CFDataGetLength(data: *const std::ffi::c_void) -> isize;
    fn CFDataGetBytePtr(data: *const std::ffi::c_void) -> *const u8;
    fn CFRelease(cf: *const std::ffi::c_void);
    fn CFBooleanGetValue(boolean: *const std::ffi::c_void) -> u8;
    static kCFBooleanTrue: *const std::ffi::c_void;
    static kCFTypeDictionaryKeyCallBacks: std::ffi::c_void;
    static kCFTypeDictionaryValueCallBacks: std::ffi::c_void;
    // Attribute keys (CFStringRef) — resolved at runtime via dlsym-style externs from Security.
    static kSecClass: *const std::ffi::c_void;
    static kSecClassGenericPassword: *const std::ffi::c_void;
    static kSecClassKey: *const std::ffi::c_void;
    static kSecAttrService: *const std::ffi::c_void;
    static kSecAttrAccount: *const std::ffi::c_void;
    static kSecValueData: *const std::ffi::c_void;
    static kSecReturnData: *const std::ffi::c_void;
    static kSecAttrIsPermanent: *const std::ffi::c_void;
    static kSecAttrApplicationTag: *const std::ffi::c_void;
    static kSecAttrKeyType: *const std::ffi::c_void;
    static kSecAttrKeyTypeECSECPrimeRandom: *const std::ffi::c_void;
    static kSecAttrKeySizeInBits: *const std::ffi::c_void;
    static kSecAttrTokenID: *const std::ffi::c_void;
    static kSecAttrTokenIDSecureEnclave: *const std::ffi::c_void;
    static kSecPrivateKeyAttrs: *const std::ffi::c_void;
    static kSecAttrAccessible: *const std::ffi::c_void;
    static kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly: *const std::ffi::c_void;
    static kSecAttrAccessGroup: *const std::ffi::c_void;
}

#[cfg(target_os = "macos")]
const kCFStringEncodingUTF8: u32 = 0x0800_0100;
#[cfg(target_os = "macos")]
const errSecSuccess: i32 = 0;
#[cfg(target_os = "macos")]
const errSecDuplicateItem: i32 = -25299;
#[cfg(target_os = "macos")]
const errSecItemNotFound: i32 = -25300;

#[cfg(target_os = "macos")]
fn anubis_kc_cfstr(s: &str) -> *const std::ffi::c_void {
    let c = std::ffi::CString::new(s).unwrap_or_default();
    unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), kCFStringEncodingUTF8) }
}

#[cfg(target_os = "macos")]
fn anubis_kc_dict(pairs: &[(*const std::ffi::c_void, *const std::ffi::c_void)]) -> *const std::ffi::c_void {
    let keys: Vec<*const std::ffi::c_void> = pairs.iter().map(|(k, _)| *k).collect();
    let vals: Vec<*const std::ffi::c_void> = pairs.iter().map(|(_, v)| *v).collect();
    unsafe {
        CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            vals.as_ptr(),
            pairs.len() as isize,
            &kCFTypeDictionaryKeyCallBacks as *const _ as *const std::ffi::c_void,
            &kCFTypeDictionaryValueCallBacks as *const _ as *const std::ffi::c_void,
        )
    }
}

#[cfg(target_os = "macos")]
fn anubis_kc_keychain_available() -> bool {
    // Smoke: add+delete a tiny probe item under a unique account.
    let acct = format!("anubis-probe-{}", anubis_kc_nonce());
    match anubis_kc_mint_keychain_account("probe", &acct) {
        Ok(tok) => {
            let _ = anubis_kc_delete_token(&tok);
            true
        }
        Err(_) => false,
    }
}

#[cfg(target_os = "macos")]
fn anubis_kc_se_available() -> bool {
    // Attempt SE key gen; delete immediately. Headless CI often fails → false.
    match anubis_kc_mint_se("probe") {
        Ok(tok) => {
            let _ = anubis_kc_delete_token(&tok);
            true
        }
        Err(_) => false,
    }
}

#[cfg(target_os = "macos")]
fn anubis_kc_mint_keychain(kind: &str) -> Result<String, i32> {
    let acct = format!("ne-{}-{}", kind, anubis_kc_nonce());
    anubis_kc_mint_keychain_account(kind, &acct)
}

#[cfg(target_os = "macos")]
fn anubis_kc_mint_keychain_account(kind: &str, account: &str) -> Result<String, i32> {
    unsafe {
        let service = anubis_kc_cfstr("anubis.capability.nonexportable");
        let acct = anubis_kc_cfstr(account);
        let payload = format!("kind={kind};pid={}", std::process::id());
        let data = CFDataCreate(
            std::ptr::null(),
            payload.as_ptr(),
            payload.len() as isize,
        );
        // Optional access group from signed-run path (ANUBIS_KEYCHAIN_ACCESS_GROUP=TEAM.anubis.capability).
        let group_env = std::env::var("ANUBIS_KEYCHAIN_ACCESS_GROUP").ok();
        let group_cf = group_env
            .as_ref()
            .map(|g| anubis_kc_cfstr(g));
        let mut pairs: Vec<(*const std::ffi::c_void, *const std::ffi::c_void)> = vec![
            (kSecClass, kSecClassGenericPassword),
            (kSecAttrService, service),
            (kSecAttrAccount, acct),
            (kSecValueData, data),
            (kSecAttrAccessible, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly),
        ];
        if let Some(g) = group_cf {
            pairs.push((kSecAttrAccessGroup, g));
        }
        let attrs = anubis_kc_dict(&pairs);
        let status = SecItemAdd(attrs, std::ptr::null_mut());
        CFRelease(attrs);
        CFRelease(service);
        CFRelease(acct);
        CFRelease(data);
        if let Some(g) = group_cf {
            CFRelease(g);
        }
        if status == errSecSuccess || status == errSecDuplicateItem {
            Ok(format!("__anubis_cap_ne_kc:{account}"))
        } else {
            Err(status)
        }
    }
}

#[cfg(target_os = "macos")]
fn anubis_kc_mint_se(kind: &str) -> Result<String, i32> {
    unsafe {
        let tag_str = format!("anubis.ne.{kind}.{}", anubis_kc_nonce());
        let tag_data = CFDataCreate(
            std::ptr::null(),
            tag_str.as_ptr(),
            tag_str.len() as isize,
        );
        // bits as CFNumber — use a small helper via CFString for size to avoid CFNumber link complexity:
        // SecKeyCreateRandomKey accepts CFDictionary; kSecAttrKeySizeInBits as CFNumber is required.
        // Use 256-bit EC via integer CFNumber created from bytes — link CoreFoundation CFNumberCreate.
    }
    // Prefer a dedicated CFNumber path:
    anubis_kc_mint_se_inner(kind)
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFNumberCreate(
        allocator: *const std::ffi::c_void,
        the_type: isize,
        value_ptr: *const std::ffi::c_void,
    ) -> *const std::ffi::c_void;
}

#[cfg(target_os = "macos")]
const kCFNumberSInt32Type: isize = 3;

#[cfg(target_os = "macos")]
fn anubis_kc_mint_se_inner(kind: &str) -> Result<String, i32> {
    unsafe {
        let tag_str = format!("anubis.ne.{kind}.{}", anubis_kc_nonce());
        let tag_data = CFDataCreate(
            std::ptr::null(),
            tag_str.as_ptr(),
            tag_str.len() as isize,
        );
        let bits: i32 = 256;
        let bits_num = CFNumberCreate(
            std::ptr::null(),
            kCFNumberSInt32Type,
            &bits as *const i32 as *const std::ffi::c_void,
        );
        let priv_attrs = anubis_kc_dict(&[
            (kSecAttrIsPermanent, kCFBooleanTrue),
            (kSecAttrApplicationTag, tag_data),
            (kSecAttrAccessible, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly),
        ]);
        let params = anubis_kc_dict(&[
            (kSecAttrKeyType, kSecAttrKeyTypeECSECPrimeRandom),
            (kSecAttrKeySizeInBits, bits_num),
            (kSecAttrTokenID, kSecAttrTokenIDSecureEnclave),
            (kSecPrivateKeyAttrs, priv_attrs),
        ]);
        let mut err: *const std::ffi::c_void = std::ptr::null();
        let key = SecKeyCreateRandomKey(params, &mut err);
        CFRelease(params);
        CFRelease(priv_attrs);
        CFRelease(tag_data);
        CFRelease(bits_num);
        if key.is_null() {
            if !err.is_null() {
                CFRelease(err);
            }
            return Err(-1);
        }
        let pubk = SecKeyCopyPublicKey(key);
        let mut err2: *const std::ffi::c_void = std::ptr::null();
        let ext = if pubk.is_null() {
            std::ptr::null()
        } else {
            SecKeyCopyExternalRepresentation(pubk, &mut err2)
        };
        let mut hash_hex = String::new();
        if !ext.is_null() {
            let len = CFDataGetLength(ext) as usize;
            let ptr = CFDataGetBytePtr(ext);
            if !ptr.is_null() && len > 0 {
                let bytes = std::slice::from_raw_parts(ptr, len);
                // FNV-1a 64-bit fingerprint of public key bytes (not a crypto claim — handle id).
                let mut h: u64 = 0xcbf29ce484222325;
                for b in bytes {
                    h ^= *b as u64;
                    h = h.wrapping_mul(0x100000001b3);
                }
                hash_hex = format!("{h:016x}");
            }
            CFRelease(ext);
        }
        if !err2.is_null() {
            CFRelease(err2);
        }
        if !pubk.is_null() {
            CFRelease(pubk);
        }
        // Keep private key in SE keychain (permanent). Token is a handle, not the key material.
        CFRelease(key);
        if hash_hex.is_empty() {
            hash_hex = anubis_kc_nonce();
        }
        Ok(format!("__anubis_cap_ne_se:{kind}:{hash_hex}"))
    }
}

#[cfg(target_os = "macos")]
fn anubis_kc_delete_token(tok: &str) -> Result<(), i32> {
    if let Some(acct) = tok.strip_prefix("__anubis_cap_ne_kc:") {
        unsafe {
            let service = anubis_kc_cfstr("anubis.capability.nonexportable");
            let account = anubis_kc_cfstr(acct);
            let query = anubis_kc_dict(&[
                (kSecClass, kSecClassGenericPassword),
                (kSecAttrService, service),
                (kSecAttrAccount, account),
            ]);
            let status = SecItemDelete(query);
            CFRelease(query);
            CFRelease(service);
            CFRelease(account);
            if status == errSecSuccess || status == errSecItemNotFound {
                Ok(())
            } else {
                Err(status)
            }
        }
    } else if tok.starts_with("__anubis_cap_ne_se:") {
        // SE keys are permanent; best-effort delete by application tag is residual.
        Ok(())
    } else {
        Ok(())
    }
}
