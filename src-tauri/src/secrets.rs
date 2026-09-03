use crate::state::AppResult;

const PREFIX: &str = "dpapi:";

#[cfg(windows)]
pub fn protect(value: &str) -> AppResult<String> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    use std::ptr;
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB},
    };

    let mut input = value.as_bytes().to_vec();
    let input_len =
        u32::try_from(input.len()).map_err(|_| "Secret is too large to protect.".to_string())?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: input_len,
        pbData: input.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    let protected = unsafe {
        CryptProtectData(
            &input,
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if protected == 0 {
        return Err("Windows could not protect a database secret with DPAPI.".to_string());
    }
    let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) };
    let encoded = URL_SAFE_NO_PAD.encode(bytes);
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(format!("{PREFIX}{encoded}"))
}

#[cfg(windows)]
pub fn unprotect(value: &str) -> AppResult<String> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    use std::ptr;
    use windows_sys::{
        core::PWSTR,
        Win32::{
            Foundation::LocalFree,
            Security::Cryptography::{
                CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
            },
        },
    };

    let encoded = value
        .strip_prefix(PREFIX)
        .ok_or_else(|| "Unsupported protected secret format.".to_string())?;
    let mut input = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| "Protected database secret is invalid.".to_string())?;
    let input_len = u32::try_from(input.len())
        .map_err(|_| "Protected database secret is too large.".to_string())?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: input_len,
        pbData: input.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    let mut description: PWSTR = ptr::null_mut();
    let unprotected = unsafe {
        CryptUnprotectData(
            &input,
            &mut description,
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if unprotected == 0 {
        return Err("Windows could not decrypt a database secret with DPAPI.".to_string());
    }
    let result = String::from_utf8(
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec(),
    )
    .map_err(|_| "Protected database secret is not valid UTF-8.".to_string());
    unsafe {
        LocalFree(output.pbData.cast());
        if !description.is_null() {
            LocalFree(description.cast());
        }
    }
    result
}

#[cfg(not(windows))]
pub fn protect(_: &str) -> AppResult<String> {
    Err("Database secret protection is available only on Windows.".to_string())
}

#[cfg(not(windows))]
pub fn unprotect(_: &str) -> AppResult<String> {
    Err("Database secret protection is available only on Windows.".to_string())
}

#[cfg(all(test, windows))]
mod tests {
    use super::{protect, unprotect};

    #[test]
    fn round_trips_a_dpapi_secret() {
        let value = "LocalStack-Pro-secret-42";
        let protected = protect(value).expect("protect secret");
        assert!(protected.starts_with("dpapi:"));
        assert_ne!(protected, value);
        assert_eq!(unprotect(&protected).expect("unprotect secret"), value);
    }
}
