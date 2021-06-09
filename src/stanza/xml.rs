use std::cell::Ref;
use std::fmt;
use std::rc::Rc;
use std::fmt::Write;
use rxml::CData;
use super::tree;

const SPECIALS: &'static [char] = &[
	'"',
	'\'',
	'<',
	'>',
	'&',
];

pub fn escape<'a, W>(out: &'a mut W, val: &'a str) -> fmt::Result
	where W: Write
{
	let mut last_index = 0;
	for (index_to_escape, special) in val.match_indices(&SPECIALS[..]) {
		if index_to_escape > last_index {
			out.write_str(&val[last_index..index_to_escape])?;
		}
		match special {
			"\"" => out.write_str("&quot;")?,
			"'" => out.write_str("&apos;")?,
			"<" => out.write_str("&lt;")?,
			">" => out.write_str("&gt;")?,
			"&" => out.write_str("&amp;")?,
			_ => panic!("unexpected special character?!"),
		}
		last_index = index_to_escape + 1
	}
	out.write_str(&val[last_index..val.len()])?;
	Ok(())
}

fn attr_escape_value<'a, W>(out: &'a mut W, val: &'a str) -> fmt::Result
	where W: Write
{
	out.write_str("'")?;
	escape(out, val)?;
	out.write_str("'")?;
	Ok(())
}

fn attr_escape<'a, W>(out: &'a mut W, name: &'a str, val: &'a str, nsid: &mut usize) -> fmt::Result
	where W: Write
{
	let (xmlns, name) = match name.find('\x01') {
		Some(offset) => (Some(&name[..offset]), &name[offset+1..]),
		None => (None, name),
	};

	if let Some(xmlns) = xmlns {
		let prefix = format!("prosody-tmp-ns{}", *nsid);
		*nsid = *nsid + 1;
		write!(out, " xmlns:{}=", prefix)?;
		attr_escape_value(out, xmlns)?;
		write!(out, " {}:{}=", prefix, name)?;
	} else {
		write!(out, " {}=", name)?;
	}
	attr_escape_value(out, val)
}

pub fn head_as_str<'a>(el: Ref<'a, tree::Element>) -> Result<String, fmt::Error> {
	let mut nsid = 0usize;
	let mut result = String::new();
	write!(result, "<{}", el.localname)?;
	if let Some(nsuri) = el.nsuri.as_ref() {
		attr_escape(&mut result, "xmlns", nsuri.as_str(), &mut nsid)?;
	}
	for (k, v) in el.attr.iter() {
		attr_escape(&mut result, k, v, &mut nsid)?;
	}
	result.write_str(">")?;
	Ok(result)
}

pub struct Formatter {
	pub indent: Option<String>,
	pub initial_level: usize,
}

struct FormatterState<'a> {
	formatter: &'a Formatter,
	depth: usize,
	newline: String,
	parent_ns: Option<Rc<CData>>,
}

impl<'a> FormatterState<'a> {
	pub fn new<'b>(formatter: &'b Formatter) -> FormatterState<'b> {
		let level = formatter.initial_level;
		let level = if level <= 1 {
			0
		} else {
			level - 1
		};
		let newline = match formatter.indent.as_ref() {
			Some(s) => format!("\n{}", s.repeat(level)),
			None => "\n".to_string(),
		};
		FormatterState{
			formatter: formatter,
			depth: 0,
			newline: newline,
			parent_ns: None,
		}
	}

	fn indent(&mut self) {
		self.depth += 1;
	}

	fn dedent(&mut self) {
		debug_assert!(self.depth > 0);
		self.depth -= 1;
	}

	fn write_indent<'b, W>(&self, indent: &'b String, f: &'b mut W) -> Result<(), fmt::Error>
		where W: Write
	{
		f.write_str(&self.newline)?;
		f.write_str(&indent.repeat(self.depth))?;
		Ok(())
	}

	fn format_node<'b, W>(&mut self, node: &'b tree::Node, f: &'b mut W) -> Result<(), fmt::Error>
		where W: Write
	{
		match node {
			tree::Node::Text(s) => escape(f, s),
			tree::Node::Element(eptr) => self.format_el(eptr.borrow(), f),
		}
	}

	fn format_el<'b, W>(&mut self, el: Ref<'b, tree::Element>, f: &'b mut W) -> Result<(), fmt::Error>
		where W: Write
	{
		write!(f, "<{}", el.localname)?;
		let mut nsid = 0usize;
		let assumed_nsuri = if self.parent_ns != el.nsuri {
			if let Some(nsuri) = el.nsuri.as_ref() {
				attr_escape(f, "xmlns", nsuri.as_str(), &mut nsid)?;
				el.nsuri.clone()
			} else if let Some(nsuri) = self.parent_ns.as_ref() {
				// XXX: we use the parent uri here, because prosody does not distinguish between that and uses more of a physical representation in its attributes.
				attr_escape(f, "xmlns", nsuri.as_str(), &mut nsid)?;
				self.parent_ns.clone()
			} else {
				None
			}
		} else {
			el.nsuri.clone()
		};
		for (k, v) in el.attr.iter() {
			attr_escape(f, k, v, &mut nsid)?;
		}
		if el.len() == 0 {
			return f.write_str("/>")
		}
		if let Some(indent) = self.formatter.indent.as_ref() {
			f.write_str(">")?;
			if el.len() == 1 && el.element_view().len() == 0 {
				// only single text node, we don’t indent those
				f.write_str(el[0].as_text().unwrap())?;
			} else {
				self.indent();
				for child in el.iter() {
					self.parent_ns = assumed_nsuri.clone();
					if let tree::Node::Text(ref s) = child {
						// whitespace-only children are ignored
						if self.formatter.indent.is_some() && s.find(|c| { !char::is_whitespace(c) }).is_none() {
							continue
						}
					}
					self.write_indent(indent, f)?;
					self.format_node(child, f)?;
				}
				self.dedent();
				self.write_indent(indent, f)?;
			}
		} else {
			f.write_str(">")?;
			for child in el.iter() {
				self.parent_ns = assumed_nsuri.clone();
				self.format_node(child, f)?;
			}
		}
		write!(f, "</{}>", el.localname)?;
		Ok(())
	}
}

impl Formatter {
	pub fn format_into<'a, W>(&self, el: Ref<'a, tree::Element>, f: &'a mut W) -> Result<(), fmt::Error>
		where W: Write
	{
		let mut state = FormatterState::new(self);
		state.format_el(el, f)
	}

	pub fn format<'a>(&self, el: Ref<'a, tree::Element>) -> Result<String, fmt::Error> {
		let mut buf = String::new();
		self.format_into(el, &mut buf)?;
		Ok(buf)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::HashMap;
	use std::convert::TryInto;

	#[test]
	fn escape_plain() {
		let mut buf = String::new();
		assert!(escape(&mut buf, "foobar 2342").is_ok());
		assert_eq!(buf, "foobar 2342");
	}

	#[test]
	fn escape_apos_begin() {
		let mut buf = String::new();
		assert!(escape(&mut buf, "'foobar 2342").is_ok());
		assert_eq!(buf, "&apos;foobar 2342");
	}

	#[test]
	fn escape_apos_end() {
		let mut buf = String::new();
		assert!(escape(&mut buf, "foobar 2342'").is_ok());
		assert_eq!(buf, "foobar 2342&apos;");
	}

	#[test]
	fn escape_double_apos() {
		let mut buf = String::new();
		assert!(escape(&mut buf, "foobar''2342").is_ok());
		assert_eq!(buf, "foobar&apos;&apos;2342");
	}

	#[test]
	fn escape_ampersand() {
		let mut buf = String::new();
		assert!(escape(&mut buf, "foobar&2342").is_ok());
		assert_eq!(buf, "foobar&amp;2342");
	}

	#[test]
	fn escape_quot() {
		let mut buf = String::new();
		assert!(escape(&mut buf, "foobar\"2342").is_ok());
		assert_eq!(buf, "foobar&quot;2342");
	}

	#[test]
	fn escape_lt() {
		let mut buf = String::new();
		assert!(escape(&mut buf, "foobar<2342").is_ok());
		assert_eq!(buf, "foobar&lt;2342");
	}

	#[test]
	fn escape_gt() {
		let mut buf = String::new();
		assert!(escape(&mut buf, "foobar>2342").is_ok());
		assert_eq!(buf, "foobar&gt;2342");
	}

	#[test]
	fn format_escapes_text_nodes() {
		let el = tree::ElementPtr::new(None, "foo".try_into().unwrap());
		el.borrow_mut().text("&bar;<baz/>fnord\"'".try_into().unwrap());
		let fmt = Formatter{ indent: None, initial_level: 0 };
		let s = fmt.format(el.borrow()).unwrap();
		assert_eq!(s, "<foo>&amp;bar;&lt;baz/&gt;fnord&quot;&apos;</foo>");
	}

	#[test]
	fn format_escapes_attribute_values() {
		let mut attr = HashMap::<String, String>::new();
		attr.insert("moo".try_into().unwrap(), "&bar;<baz/>fnord\"'".try_into().unwrap());
		let el = tree::ElementPtr::new_with_attr(None, "foo".try_into().unwrap(), Some(attr));
		let fmt = Formatter{ indent: None, initial_level: 0 };
		let s = fmt.format(el.borrow()).unwrap();
		assert_eq!(s, "<foo moo='&amp;bar;&lt;baz/&gt;fnord&quot;&apos;'/>");
	}

	#[test]
	fn format_handles_namespaces() {
		let ns1 = Rc::new(CData::from_string("uri:foo".try_into().unwrap()).unwrap());
		let ns2 = Rc::new(CData::from_string("uri:bar".try_into().unwrap()).unwrap());
		let mut el = tree::ElementPtr::new_with_attr(
			Some(ns1.clone()),
			"foo".try_into().unwrap(),
			None,
		);
		let c1 = el.borrow_mut().tag(Some(ns1.clone()), "child".try_into().unwrap(), None);
		let c2 = el.borrow_mut().tag(Some(ns2.clone()), "child".try_into().unwrap(), None);
		let fmt = Formatter{ indent: None, initial_level: 0 };
		let s = fmt.format(el.borrow()).unwrap();
		assert_eq!(s, "<foo xmlns='uri:foo'><child/><child xmlns='uri:bar'/></foo>");
	}
}
