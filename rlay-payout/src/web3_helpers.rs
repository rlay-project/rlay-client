use rustc_hex::ToHex;

pub struct HexString<'a> {
    pub inner: &'a [u8],
}

impl<'a> HexString<'a> {
    pub fn fmt(bytes: &'a [u8]) -> String {
        let hex: String = bytes.to_hex();
        format!("0x{}", &hex)
    }

    pub fn wrap(bytes: &'a [u8]) -> Self {
        HexString { inner: bytes }
    }

    pub fn wrap_option(bytes: Option<&'a Vec<u8>>) -> Option<Self> {
        match bytes {
            Some(bytes) => Some(HexString { inner: bytes }),
            None => None,
        }
    }
}

impl<'a> ::serde::Serialize for HexString<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        Ok(serializer.serialize_str(&Self::fmt(self.inner))?)
    }
}
