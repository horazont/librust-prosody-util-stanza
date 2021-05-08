use std::cell::Ref;
use std::collections::HashMap;

use crate::tree;

pub const XMLNS_XMPP_STANZAS: &str = "urn:ietf:params:xml:ns:xmpp-stanzas";

pub fn make_reply<'a>(el: Ref<'a, tree::Element>) -> tree::ElementPtr {
	let mut attr = HashMap::new();
	match el.attr.get("id") {
		Some(v) => {
			attr.insert("id".to_string(), v.clone());
		},
		_ => (),
	};
	match el.attr.get("from") {
		Some(v) => {
			attr.insert("to".to_string(), v.clone());
		},
		_ => (),
	};
	match el.attr.get("to") {
		Some(v) => {
			attr.insert("from".to_string(), v.clone());
		},
		_ => (),
	};

	if el.localname == "iq" {
		attr.insert("type".to_string(), "result".to_string());
	} else {
		match el.attr.get("type") {
			Some(v) => {
				attr.insert("type".to_string(), v.clone());
			},
			_ => (),
		};
	}

	tree::ElementPtr::new_with_attr(
		el.localname.clone(),
		Some(attr),
	)
}

pub struct ElementSelector {
	filter_by_name: bool,
	name: String,
	allow_absent_xmlns: bool,
	match_xmlns: bool,
	xmlns: String,
}

impl ElementSelector {
	pub fn select_inside_parent<'a>(parent: Ref<'a, tree::Element>, name: Option<String>, xmlns: Option<String>) -> ElementSelector {
		Self::select_inside_xmlns(parent.attr.get("xmlns"), name, xmlns)
	}

	pub fn select_inside_xmlns<'a>(default_xmlns: Option<&'a String>, name: Option<String>, xmlns: Option<String>) -> ElementSelector {
		let (filter_by_name, name) = match name {
			Some(n) => (true, n.clone()),
			None => (false, "".to_string()),
		};

		let (allow_absent_xmlns, match_xmlns, xmlns) = match xmlns {
			Some(ns) => match default_xmlns {
				Some(default_ns) => (*default_ns == ns, true, ns.clone()),
				None => (false, true, ns.clone()),
			},
			None => match default_xmlns {
				Some(ns) => (true, true, ns.clone()),
				None => (true, false, "".to_string()),
			},
		};

		ElementSelector{
			filter_by_name: filter_by_name,
			name: name,
			allow_absent_xmlns: allow_absent_xmlns,
			match_xmlns: match_xmlns,
			xmlns: xmlns,
		}
	}

	pub fn select<'a>(&self, element: Ref<'a, tree::Element>) -> bool {
		self.select_str(
			&element.localname,
			element.attr.get("xmlns"),
		)
	}

	pub fn select_str<'a>(&self, name: &'a String, xmlns: Option<&'a String>) -> bool {
		if self.filter_by_name && *name != self.name {
			return false;
		}

		match xmlns {
			// xmlns_selector == None && parent.xmlns != None && element.xmlns == parent.xmlns
			// xmlns_selector != None && element.xmlns == xmlns_selector
			Some(xmlns) => self.match_xmlns && self.xmlns == *xmlns,
			// xmlns_selector == None && parent.xmlns == None && element.xmlns == None
			None => self.allow_absent_xmlns,
		}
	}

	pub fn find_first_child<'a, T>(&self, iter: T) -> Option<tree::ElementPtr>
		where T: Iterator<Item = tree::ElementPtr>
	{
		for child in iter {
			if self.select(Ref::clone(&child.borrow())) {
				return Some(child);
			}
		}
		None
	}
}

type ErrorInfo = (String, String, Option<String>, Option<tree::ElementPtr>);

/// Extract ErrorInfo out of a stanza error.
///
/// Note that this expects the `<error/>` element as argument, **not** the
/// parent stanza. Use extract_error for that.
pub fn extract_error_info<'a>(el: Ref<'a, tree::Element>) -> Option<ErrorInfo> {
	let type_ = el.attr.get("type")?;
	let mut condition = "undefined-condition".to_string();
	let mut text: Option<String> = None;
	let mut appdef: Option<tree::ElementPtr> = None;

	for child_el_ptr in el.iter_children() {
		let child_el = child_el_ptr.borrow();
		match child_el.attr.get("xmlns") {
			Some(ns) => {
				if ns == XMLNS_XMPP_STANZAS {
					if child_el.localname == "text" {
						text = match child_el.get_text() {
							Some(s) => Some(s.clone()),
							None => None,
						};
					} else {
						condition = child_el.localname.clone();
					}
				} else {
					appdef = Some(child_el_ptr.clone());
				}
			},
			_ => (),
		}
	}

	Some((type_.clone(), condition, text, appdef))
}

/// Return the error info from the <error/> element inside the given stanza,
/// if any.
pub fn extract_error<'a>(st: Ref<'a, tree::Element>) -> Option<ErrorInfo> {
	let error_selector = ElementSelector::select_inside_parent(Ref::clone(&st), Some("error".to_string()), None);
	let error_child = error_selector.find_first_child(st.iter_children())?;
	extract_error_info(error_child.borrow())
}

pub fn make_error_reply<'a>(st: Ref<'a, tree::Element>, type_: String, condition: String, text: Option<String>, by: Option<String>) -> Result<tree::ElementPtr, String> {
	let reply_ptr = {
		match st.attr.get("type") {
			Some(s) => match s.as_str() {
				"error" => return Err("bad argument to make_error_reply: got stanza of type error which must not be replied to".to_string()),
				_ => (),
			},
			None => (),
		};
		make_reply(st)
	};
	{
		let err_ptr = {
			let mut reply = reply_ptr.borrow_mut();
			reply.attr.insert("type".to_string(), "error".to_string());
			reply.tag("error".to_string(), None)
		};
		let mut err = err_ptr.borrow_mut();
		err.attr.insert("type".to_string(), type_);
		match by {
			Some(by) => { err.attr.insert("by".to_string(), by); },
			_ => (),
		};
		err.tag(condition, None).borrow_mut().attr.insert("xmlns".to_string(), XMLNS_XMPP_STANZAS.to_string());
		match text {
			Some(text) => {
				let text_el_ptr = err.tag("text".to_string(), None);
				let mut text_el = text_el_ptr.borrow_mut();
				text_el.attr.insert("xmlns".to_string(), XMLNS_XMPP_STANZAS.to_string());
				text_el.text(text);
			},
			_ => (),
		}
	}
	Ok(reply_ptr)
}

pub fn find_first_child<'a>(el: Ref<'a, tree::Element>, name: Option<String>, xmlns: Option<String>) -> Option<tree::ElementPtr> {
	let selector = ElementSelector::select_inside_parent(
		Ref::clone(&el),
		name,
		xmlns,
	);
	selector.find_first_child(el.iter_children())
}

#[cfg(test)]
mod tests {
	use super::*;

	fn mkerrorattr(typename: String) -> HashMap<String, String> {
		let mut result = HashMap::new();
		result.insert("type".to_string(), typename);
		result
	}

	fn mkxmlnsattr(ns: String) -> HashMap<String, String> {
		let mut result = HashMap::new();
		result.insert("xmlns".to_string(), ns);
		result
	}

	#[test]
	fn elementselector_match_by_name() {
		let sel = ElementSelector::select_inside_xmlns(None, Some("foo".to_string()), None);
		assert!(sel.select_str(&"foo".to_string(), None));
		assert!(!sel.select_str(&"bar".to_string(), None));
	}

	#[test]
	fn elementselector_match_by_parent_xmlns() {
		let sel = ElementSelector::select_inside_xmlns(Some(&"urn:foo".to_string()), None, None);
		assert!(sel.select_str(&"foo".to_string(), Some(&"urn:foo".to_string())));
		assert!(sel.select_str(&"foo".to_string(), None));
		assert!(!sel.select_str(&"foo".to_string(), Some(&"urn:bar".to_string())));
	}

	#[test]
	fn elementselector_match_by_absent_parent_xmlns() {
		let sel = ElementSelector::select_inside_xmlns(None, None, None);
		assert!(!sel.select_str(&"foo".to_string(), Some(&"urn:foo".to_string())));
		assert!(sel.select_str(&"foo".to_string(), None));
		assert!(!sel.select_str(&"foo".to_string(), Some(&"urn:bar".to_string())));
	}

	#[test]
	fn elementselector_match_by_explicit_xmlns() {
		let sel = ElementSelector::select_inside_xmlns(Some(&"urn:foo".to_string()), None, Some("urn:bar".to_string()));
		assert!(!sel.select_str(&"foo".to_string(), Some(&"urn:foo".to_string())));
		assert!(!sel.select_str(&"foo".to_string(), None));
		assert!(sel.select_str(&"foo".to_string(), Some(&"urn:bar".to_string())));
	}

	#[test]
	fn elementselector_match_by_name_and_xmlns() {
		let sel = ElementSelector::select_inside_xmlns(Some(&"jabber:client".to_string()), Some("message".to_string()), Some("jabber:client".to_string()));
		assert!(sel.select_str(&"message".to_string(), Some(&"jabber:client".to_string())));
		assert!(sel.select_str(&"message".to_string(), None));
		assert!(!sel.select_str(&"message".to_string(), Some(&"jabber:server".to_string())));
		assert!(!sel.select_str(&"iq".to_string(), Some(&"jabber:client".to_string())));
		assert!(!sel.select_str(&"iq".to_string(), None));
		assert!(!sel.select_str(&"iq".to_string(), Some(&"jabber:server".to_string())));
	}

	#[test]
	fn extract_error_info_extracts_type_and_defaults_to_undef_condition() {
		let e = tree::ElementPtr::new_with_attr(
			"error".to_string(),
			Some(mkerrorattr("error type".to_string())),
		);
		let (type_, condition, text, extra) = extract_error_info(e.borrow()).unwrap();
		assert_eq!(type_, "error type");
		assert_eq!(condition, "undefined-condition");
		assert!(text.is_none());
		assert!(extra.is_none());
	}

	#[test]
	fn extract_error_info_extracts_type_and_condition() {
		let e = tree::ElementPtr::new_with_attr(
			"error".to_string(),
			Some(mkerrorattr("error type".to_string())),
		);
		e.borrow_mut().tag("random-condition".to_string(), Some(mkxmlnsattr(XMLNS_XMPP_STANZAS.to_string())));
		let (type_, condition, text, extra) = extract_error_info(e.borrow()).unwrap();
		assert_eq!(type_, "error type");
		assert_eq!(condition, "random-condition");
		assert!(text.is_none());
		assert!(extra.is_none());
	}

	#[test]
	fn extract_error_info_extracts_text() {
		let e = tree::ElementPtr::new_with_attr(
			"error".to_string(),
			Some(mkerrorattr("error type".to_string())),
		);
		e.borrow_mut().tag("random-condition".to_string(), Some(mkxmlnsattr(XMLNS_XMPP_STANZAS.to_string())));
		e.borrow_mut().tag("text".to_string(), Some(mkxmlnsattr(XMLNS_XMPP_STANZAS.to_string()))).borrow_mut().text("foobar 2342".to_string());
		let (type_, condition, text, extra) = extract_error_info(e.borrow()).unwrap();
		assert_eq!(type_, "error type");
		assert_eq!(condition, "random-condition");
		assert_eq!(text.unwrap(), "foobar 2342");
		assert!(extra.is_none());
	}

	#[test]
	fn extract_error_info_extracts_application_defined_condition_el() {
		let e = tree::ElementPtr::new_with_attr(
			"error".to_string(),
			Some(mkerrorattr("error type".to_string())),
		);
		e.borrow_mut().tag("random-condition".to_string(), Some(mkxmlnsattr(XMLNS_XMPP_STANZAS.to_string())));
		e.borrow_mut().tag("text".to_string(), Some(mkxmlnsattr(XMLNS_XMPP_STANZAS.to_string()))).borrow_mut().text("foobar 2342".to_string());
		let appdef_el = e.borrow_mut().tag("appdefcond".to_string(), Some(mkxmlnsattr("urn:uuid:5cf726d1-5be8-44bb-b14a-62880f783ac9".to_string())));
		let (type_, condition, text, extra) = extract_error_info(e.borrow()).unwrap();
		assert_eq!(type_, "error type");
		assert_eq!(condition, "random-condition");
		assert_eq!(text.unwrap(), "foobar 2342");
		assert!(tree::ElementPtr::ptr_eq(&extra.unwrap(), &appdef_el));
	}

	#[test]
	fn extract_error_from_stanza() {
		let st = tree::ElementPtr::new_with_attr(
			"message".to_string(),
			Some(mkxmlnsattr("jabber:client".to_string())),
		);
		{
			let e = st.borrow_mut().tag("error".to_string(), Some(mkerrorattr("wait".to_string())));
			e.borrow_mut().tag("remote-server-not-found".to_string(), Some(mkxmlnsattr(XMLNS_XMPP_STANZAS.to_string())));
		}

		let (type_, condition, text, extra) = extract_error(st.borrow()).unwrap();
		assert_eq!(type_, "wait");
		assert_eq!(condition, "remote-server-not-found");
		assert!(text.is_none());
		assert!(extra.is_none());
	}

	#[test]
	fn make_error_reply_sets_error_type() {
		let st = tree::ElementPtr::new_with_attr(
			"message".to_string(),
			None,
		);
		let reply = make_error_reply(st.borrow(), "cancel".to_string(), "undefined-condition".to_string(), None, None);
		assert!(reply.is_ok());
		let reply = reply.unwrap();
		assert_eq!(reply.borrow().attr.get("type").unwrap(), "error");
		assert_eq!(reply.borrow()[0].as_element_ptr().unwrap().borrow().attr.get("type").unwrap(), "cancel");
	}

	#[test]
	fn extract_error_can_extract_from_make_error_reply_result() {
		let st = tree::ElementPtr::new_with_attr(
			"message".to_string(),
			None,
		);
		let reply = make_error_reply(st.borrow(), "cancel".to_string(), "some-condition".to_string(), Some("error text".to_string()), Some("origin".to_string()));
		assert!(reply.is_ok());
		let (type_, condition, text, extra) = extract_error(reply.unwrap().borrow()).unwrap();
		assert_eq!(type_, "cancel");
		assert_eq!(condition, "some-condition");
		assert!(text.is_some());
		assert_eq!(text.unwrap(), "error text");
		assert!(extra.is_none());
	}

	#[test]
	fn extract_error_can_extract_from_make_error_reply_result_with_appinfo() {
		let st = tree::ElementPtr::new_with_attr(
			"message".to_string(),
			None,
		);
		let reply = make_error_reply(st.borrow(), "cancel".to_string(), "some-condition".to_string(), Some("error text".to_string()), Some("origin".to_string()));
		assert!(reply.is_ok());
		let reply = reply.unwrap();
		let custom_condition = {
			let custom_el_ptr = reply.borrow()[0].as_element_ptr().unwrap().borrow_mut().tag(
				"custom-condition".to_string(),
				None,
			);
			{
				let mut custom_el = custom_el_ptr.borrow_mut();
				custom_el.attr.insert("xmlns".to_string(), "urn:uuid:23d5821c-0141-418c-aa94-665ae2649b7c".to_string());
			}
			custom_el_ptr
		};
		let (type_, condition, text, extra) = extract_error(reply.borrow()).unwrap();
		assert_eq!(type_, "cancel");
		assert_eq!(condition, "some-condition");
		assert!(text.is_some());
		assert_eq!(text.unwrap(), "error text");
		assert!(extra.is_some());
		assert!(tree::ElementPtr::ptr_eq(&extra.unwrap(), &custom_condition));
	}
}
