//! Seals the Graph refresh token with DPAPI, so only this Windows user on this machine can
//! read it back.

#[cfg(windows)]
mod imp {
    use std::ptr;

    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    };

    fn run(data: &[u8], protect: bool) -> Option<Vec<u8>> {
        let input = CRYPT_INTEGER_BLOB { cbData: u32::try_from(data.len()).ok()?, pbData: data.as_ptr().cast_mut() };
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: ptr::null_mut() };
        // SAFETY: `input` points at `data`, which outlives the call; DPAPI only reads it. On
        // success `out` holds a LocalAlloc'd buffer that we copy and then free exactly once.
        let ok = unsafe {
            if protect {
                CryptProtectData(&input, ptr::null(), ptr::null(), ptr::null(), ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
            } else {
                CryptUnprotectData(&input, ptr::null_mut(), ptr::null(), ptr::null(), ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
            }
        };
        if ok == 0 || out.pbData.is_null() {
            return None;
        }
        // SAFETY: DPAPI returned `cbData` valid bytes at `pbData`.
        let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
        // SAFETY: `pbData` came from DPAPI's LocalAlloc and isn't used after this.
        unsafe { LocalFree(out.pbData.cast()) };
        Some(bytes)
    }

    pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
        run(data, true)
    }

    pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
        run(data, false)
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
        Some(data.to_vec())
    }

    pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
        Some(data.to_vec())
    }
}

pub use imp::{protect, unprotect};

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn round_trips() {
        let sealed = super::protect(b"refresh-me").unwrap();
        assert_ne!(sealed, b"refresh-me");
        assert_eq!(super::unprotect(&sealed).unwrap(), b"refresh-me");
    }
}
