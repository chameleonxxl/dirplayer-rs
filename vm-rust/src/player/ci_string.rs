use std::borrow::Borrow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;

/// Borrowed case-insensitive string slice.
/// Hashes and compares based on ASCII-lowercased content,
/// but preserves original case for display.
#[repr(transparent)]
pub struct CiStr(str);

impl CiStr {
    #[inline]
    pub fn new(s: &str) -> &CiStr {
        unsafe { &*(s as *const str as *const CiStr) }
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Hash for CiStr {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        for b in self.0.bytes() {
            state.write_u8(b.to_ascii_lowercase());
        }
        state.write_u8(0xff);
    }
}

impl PartialEq for CiStr {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl Eq for CiStr {}

impl fmt::Display for CiStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Debug for CiStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl ToOwned for CiStr {
    type Owned = CiString;

    fn to_owned(&self) -> CiString {
        CiString(self.0.to_owned())
    }
}

/// Owned case-insensitive string.
/// Hashes and compares based on ASCII-lowercased content,
/// but preserves original case for display.
#[derive(Clone)]
pub struct CiString(String);

impl CiString {
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[inline]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl Hash for CiString {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        CiStr::new(&self.0).hash(state)
    }
}

impl PartialEq for CiString {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl Eq for CiString {}

impl Borrow<CiStr> for CiString {
    #[inline]
    fn borrow(&self) -> &CiStr {
        CiStr::new(&self.0)
    }
}

impl Deref for CiString {
    type Target = CiStr;

    #[inline]
    fn deref(&self) -> &CiStr {
        CiStr::new(&self.0)
    }
}

impl From<String> for CiString {
    #[inline]
    fn from(s: String) -> Self {
        CiString(s)
    }
}

impl From<&str> for CiString {
    #[inline]
    fn from(s: &str) -> Self {
        CiString(s.to_owned())
    }
}

impl fmt::Display for CiString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Debug for CiString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
