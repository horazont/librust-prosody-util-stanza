use bstr::BStr;
use std::fmt;

#[derive(PartialEq, Debug)]
pub enum TextConstraintError {
	EmptyString,
	InvalidCharacter(usize, char),
	InvalidUtf8,
}

// TODO: use a struct wrapper around String in order to typesafely check that
// strings conform to certain requirements?

impl fmt::Display for TextConstraintError {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			TextConstraintError::InvalidUtf8 => write!(f, "invalid utf8"),
			TextConstraintError::InvalidCharacter(pos, _) => write!(f, "invalid character at position {}", pos),
			TextConstraintError::EmptyString => write!(f, "empty string"),
		}
	}
}

pub fn convert_xml_element_name(raw: &BStr) -> Result<String, TextConstraintError> {
	let s = match String::from_utf8(raw.to_vec()) {
		Ok(s) => s,
		Err(_) => return Err(TextConstraintError::InvalidUtf8),
	};
	match check_xml_element_name(s.as_str(), false) {
		Some(err) => Err(err),
		None => Ok(s),
	}
}

pub fn convert_xml_attribute_name(raw: &BStr) -> Result<String, TextConstraintError> {
	let s = match String::from_utf8(raw.to_vec()) {
		Ok(s) => s,
		Err(_) => return Err(TextConstraintError::InvalidUtf8),
	};
	match check_xml_element_name(s.as_str(), true) {
		Some(err) => Err(err),
		None => Ok(s),
	}
}

pub fn convert_xml_cdata(raw: &BStr) -> Result<String, TextConstraintError> {
	let s = match String::from_utf8(raw.to_vec()) {
		Ok(s) => s,
		Err(_) => return Err(TextConstraintError::InvalidUtf8),
	};
	match check_xml_cdata(s.as_str()) {
		Some(err) => Err(err),
		None => Ok(s),
	}
}

// start to end (incl., because some of our edge points are not valid chars
// in rust)
struct CodepointRange(char, char);

// XML 1.0 § 2.2
const VALID_XML_CDATA_RANGES: &'static [CodepointRange] = &[
	CodepointRange('\x09', '\x0a'),
	CodepointRange('\x0d', '\x0d'),
	CodepointRange('\u{0020}', '\u{d7ff}'),
	CodepointRange('\u{e000}', '\u{fffd}'),
	CodepointRange('\u{10000}', '\u{10ffff}'),
];


// XML 1.0 § 2.3 [4]
const VALID_XML_NAME_START_RANGES: &'static [CodepointRange] = &[
	CodepointRange(':', ':'),
	CodepointRange('A', 'Z'),
	CodepointRange('_', '_'),
	CodepointRange('a', 'z'),
	CodepointRange('\u{c0}', '\u{d6}'),
	CodepointRange('\u{d8}', '\u{f6}'),
	CodepointRange('\u{f8}', '\u{2ff}'),
	CodepointRange('\u{370}', '\u{37d}'),
	CodepointRange('\u{37f}', '\u{1fff}'),
	CodepointRange('\u{200c}', '\u{200d}'),
	CodepointRange('\u{2070}', '\u{218f}'),
	CodepointRange('\u{2c00}', '\u{2fef}'),
	CodepointRange('\u{3001}', '\u{d7ff}'),
	CodepointRange('\u{f900}', '\u{fdcf}'),
	CodepointRange('\u{10000}', '\u{effff}'),
];


// XML 1.0 § 2.3 [4a]
const VALID_XML_NAME_RANGES: &'static [CodepointRange] = &[
	CodepointRange(':', ':'),
	CodepointRange('-', '-'),
	CodepointRange('.', '.'),
	CodepointRange('A', 'Z'),
	CodepointRange('_', '_'),
	CodepointRange('0', '9'),
	CodepointRange('a', 'z'),
	CodepointRange('\u{b7}', '\u{b7}'),
	CodepointRange('\u{c0}', '\u{d6}'),
	CodepointRange('\u{d8}', '\u{f6}'),
	CodepointRange('\u{f8}', '\u{2ff}'),
	CodepointRange('\u{300}', '\u{36f}'),
	CodepointRange('\u{370}', '\u{37d}'),
	CodepointRange('\u{37f}', '\u{1fff}'),
	CodepointRange('\u{200c}', '\u{200d}'),
	CodepointRange('\u{203f}', '\u{2040}'),
	CodepointRange('\u{2070}', '\u{218f}'),
	CodepointRange('\u{2c00}', '\u{2fef}'),
	CodepointRange('\u{3001}', '\u{d7ff}'),
	CodepointRange('\u{f900}', '\u{fdcf}'),
	CodepointRange('\u{10000}', '\u{effff}'),
];

impl CodepointRange {
	pub fn contains(&self, c: char) -> bool {
		return (self.0 <= c) && (c <= self.1)
	}
}

pub fn check_valid_cdata_char(c: char) -> bool {
	for range in VALID_XML_CDATA_RANGES.iter() {
		if range.contains(c) {
			return true;
		}
	}
	return false;
}

pub fn check_valid_name_start_char(c: char) -> bool {
	for range in VALID_XML_NAME_START_RANGES.iter() {
		if range.contains(c) {
			return true;
		}
	}
	return false;
}

pub fn check_valid_name_char(c: char) -> bool {
	for range in VALID_XML_NAME_RANGES.iter() {
		if range.contains(c) {
			return true;
		}
	}
	return false;
}

pub fn check_xml_element_name(s: &str, attribute_hack: bool) -> Option<TextConstraintError> {
	let mut iterator = s.chars().enumerate();
	let (i, codepoint) = match iterator.next() {
		Some((i, codepoint)) => (i, codepoint),
		None => return Some(TextConstraintError::EmptyString),
	};
	if !check_valid_name_start_char(codepoint) {
		return Some(TextConstraintError::InvalidCharacter(i, codepoint));
	}

	for (i, codepoint) in iterator {
		if attribute_hack && codepoint == '\x01' {
			// prosody uses `xmlns\x01attribute-name` as internal storage for namespaced attributes... don’t ask me.
			continue
		}

		if !check_valid_name_char(codepoint) {
			return Some(TextConstraintError::InvalidCharacter(i, codepoint));
		}
	}
	None
}

pub fn check_xml_cdata(s: &str) -> Option<TextConstraintError> {
	for (i, codepoint) in s.chars().enumerate() {
		if !check_valid_cdata_char(codepoint) {
			return Some(TextConstraintError::InvalidCharacter(i, codepoint));
		}
	}
	None
}

#[cfg(test)]
#[test]
fn reject_empty_string_for_elements() {
	assert_eq!(check_xml_element_name("", false), Some(TextConstraintError::EmptyString));
}
#[cfg(test)]
#[test]
fn reject_element_name_starting_with_number() {
	assert_eq!(check_xml_element_name("0foo", false), Some(TextConstraintError::InvalidCharacter(0, '0')));
}
