use std::convert::TryFrom;
use std::borrow::Borrow;
use rxml::{NCName, NCNameStr, NameStr, CDataStr};
use smartstring::alias::{String as SmartString};

pub const XMLNS_XML: &'static str = "http://www.w3.org/XML/1998/namespace";
pub const XMLNS_XML_PREFIX: &'static str = "http://www.w3.org/XML/1998/namespace\x01";

/// A libexpat-compatible attribute name
///
/// Such an attribute name consists of an [`rxml::NCName`], optionally
/// prefixed with a `\1` prefixed by the namespace URI.
#[derive(Hash, PartialEq, Debug, Clone)]
pub struct AttrName(SmartString);

impl Eq for AttrName {}

impl AttrName {
	pub fn compose<T: AsRef<CDataStr>, U: Into<NCName>>(ns: Option<T>, name: U) -> AttrName {
		let name = name.into();
		match ns {
			Some(ns) => {
				let ns = ns.as_ref();
				let mut buf = String::with_capacity(ns.len() + name.len() + 1);
				buf.push_str(ns);
				buf.push_str("\x01");
				buf.push_str(&name);
				AttrName(buf.into())
			},
			None => AttrName(name.into()),
		}
	}

	pub fn local_only<T: Into<NCName>>(name: T) -> AttrName {
		AttrName(name.into().into())
	}

	pub fn decompose(&self) -> (Option<&CDataStr>, &NameStr) {
		match self.0.find('\x01') {
			Some(delim_pos) => {
				let (nsuri, localname) = self.0.split_at(delim_pos);
				let localname = &localname[1..];
				(
					Some(unsafe { CDataStr::from_str_unchecked(nsuri) }),
					unsafe { NameStr::from_str_unchecked(localname) },
				)
			},
			None => (
				None,
				unsafe { NameStr::from_str_unchecked(&self.0) },
			),
		}
	}

	pub fn as_string(self) -> String {
		self.0.to_string()
	}

	fn from_str<T: AsRef<str>>(s: T) -> Result<AttrName, &'static str> {
		let s = s.as_ref();
		if s.starts_with("xml:") {
			// special case for prosody compat
			let localname = &s[4..];
			let localname = match NCNameStr::from_str(localname) {
				Ok(v) => v,
				Err(_) => return Err("local name is not well formed"),
			};
			return Ok(Self::compose(Some(CDataStr::from_str(XMLNS_XML).unwrap()), localname));
		}
		if s.starts_with("xmlns:") {
			// keep as-is
			let localname = &s[6..];
			match NCNameStr::from_str(localname) {
				Ok(_) => (),
				Err(_) => return Err("namespace prefix is not well formed"),
			};
			return Ok(AttrName(s.into()))
		}
		match s.find('\x01') {
			Some(first_delim) => {
				let (nsuri, remainder) = s.split_at(first_delim);
				if remainder.len() <= 1 {
					return Err("local name (following \\1 separator) is empty")
				}
				let remainder = &remainder[1..];
				let nsuri = match CDataStr::from_str(nsuri) {
					Ok(v) => v,
					Err(_) => return Err("nsuri is not well formed"),
				};
				let remainder = match NCNameStr::from_str(remainder) {
					Ok(v) => v,
					Err(_) => return Err("local name is not well formed"),
				};
				Ok(Self::compose(Some(nsuri), remainder))
			},
			None => {
				let localname = match NCNameStr::from_str(s) {
					Ok(v) => v,
					Err(_) => return Err("local name is not well formed"),
				};
				Ok(Self::local_only(localname))
			},
		}
	}

	pub fn from_string<T: Into<SmartString>>(s: T) -> Result<AttrName, &'static str> {
		let mut s = s.into();
		if s.starts_with("xml:") {
			// special case for prosody compat
			s.replace_range(..4, "");
			let localname = match NCName::from_string(s.to_string()) {
				Ok(v) => v,
				Err(_) => return Err("local name is not well formed"),
			};
			return Ok(Self::compose(Some(CDataStr::from_str(XMLNS_XML).unwrap()), localname));
		}
		if s.starts_with("xmlns:") {
			{
				let localname = &s[6..];
				match NCNameStr::from_str(localname) {
					Ok(_) => (),
					Err(_) => return Err("namespace prefix is not well formed"),
				};
			}
			return Ok(AttrName(s))
		}
		match s.find('\x01') {
			Some(first_delim) => {
				let (nsuri, remainder) = s.split_at(first_delim);
				if remainder.len() <= 1 {
					return Err("local name (following \\1 separator) is empty")
				}
				let remainder = &remainder[1..];
				let nsuri = match CDataStr::from_str(nsuri) {
					Ok(v) => v,
					Err(_) => return Err("nsuri is not well formed"),
				};
				let remainder = match NCNameStr::from_str(remainder) {
					Ok(v) => v,
					Err(_) => return Err("local name is not well formed"),
				};
				Ok(Self::compose(Some(nsuri), remainder))
			},
			None => {
				let localname = match NCName::from_string(s.to_string()) {
					Ok(v) => v,
					Err(_) => return Err("local name is not well formed"),
				};
				Ok(AttrName(localname.into()))
			},
		}
	}

	pub unsafe fn from_str_unsafe<T: AsRef<str>>(s: T) -> AttrName {
		AttrName(s.as_ref().into())
	}

	pub unsafe fn from_string_unsafe<T: Into<String>>(s: T) -> AttrName {
		AttrName(s.into().into())
	}
}

impl From<NCName> for AttrName {
	fn from(other: NCName) -> AttrName {
		AttrName::local_only(other)
	}
}

impl PartialEq<&str> for AttrName {
	fn eq(&self, other: &&str) -> bool {
		&self.0 == other
	}
}

impl Borrow<str> for AttrName {
	fn borrow(&self) -> &str {
		&self.0
	}
}

impl AsRef<str> for AttrName {
	fn as_ref(&self) -> &str {
		&self.0
	}
}

impl TryFrom<&str> for AttrName {
	type Error = &'static str;

	fn try_from(other: &str) -> Result<Self, Self::Error> {
		Self::from_str(other)
	}
}
