use std::collections::HashMap;
use std::cell::Ref;

use crate::tree;
use crate::path;

pub struct Stanza {
	root: tree::ElementPtr,
	cursor: path::ElementPath,
}

impl Stanza {
	pub fn new(name: String, attr: Option<HashMap<String, String>>) -> Stanza {
		Stanza::wrap(
			tree::ElementPtr::wrap(tree::Element::new_with_attr(name, attr)),
		)
	}

	pub fn wrap(ptr: tree::ElementPtr) -> Stanza {
		Stanza{
			root: ptr,
			cursor: path::ElementPath::new(),
		}
	}

	pub fn try_deref(&self) -> Option<tree::ElementPtr> {
		self.cursor.deref_on(self.root.clone())
	}

	pub fn root<'a>(&'a self) -> Ref<'a, tree::Element> {
		self.root.borrow()
	}

	pub fn root_ptr(&self) -> tree::ElementPtr {
		self.root.clone()
	}

	pub fn tag(&mut self, name: String, attr: Option<HashMap<String, String>>) -> Option<tree::ElementPtr> {
		let parent_ptr = self.cursor.deref_on(self.root.clone())?;
		let mut parent = parent_ptr.borrow_mut();
		let new_index = parent.len();
		self.cursor.down(new_index);
		Some(parent.tag(name, attr))
	}

	pub fn text(&mut self, data: String) -> bool {
		let parent_ptr = match self.cursor.deref_on(self.root.clone()) {
			Some(p) => p,
			None => return false,
		};
		let mut parent = parent_ptr.borrow_mut();
		parent.text(data);
		true
	}

	pub fn up(&mut self) {
		self.cursor.up();
	}

	pub fn reset(&mut self) {
		self.cursor.reset();
	}

	pub fn deep_clone(&self) -> Stanza {
		Stanza::wrap(self.root.deep_clone())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn stanza_new_cursor_at_root() {
		let st = Stanza::new("message".to_string(), None);
		let root = st.try_deref();
		assert!(root.is_some());
		let root = root.unwrap();
		assert_eq!(root.borrow().localname, "message");
	}

	#[test]
	fn stanza_tag_descends() {
		let mut st = Stanza::new("message".to_string(), None);
		let body = st.tag("body".to_string(), None);
		assert!(body.is_some());
		let body = body.unwrap();
		let body_derefd = st.try_deref();
		assert!(body_derefd.is_some());
		let body_derefd = body_derefd.unwrap();
		assert!(tree::ElementPtr::ptr_eq(&body, &body_derefd));
	}

	#[test]
	fn stanza_text_does_not_descend() {
		let mut st = Stanza::new("body".to_string(), None);
		st.text("foo".to_string());
		let root = st.try_deref();
		assert!(root.is_some());
		let root = root.unwrap();
		assert_eq!(root.borrow().localname, "body");
	}

	#[test]
	fn stanza_tag_inserts_at_cursor() {
		let mut st = Stanza::new("iq".to_string(), None);
		st.tag("query".to_string(), None);
		st.tag("item".to_string(), None);
		assert_eq!(st.root().len(), 1);
		assert_eq!(st.root()[0].as_element_ptr().unwrap().borrow().len(), 1);
		assert_eq!(st.root()[0].as_element_ptr().unwrap().borrow()[0].as_element_ptr().unwrap().borrow().len(), 0);
	}

	#[test]
	fn stanza_up_moves_cursor() {
		let mut st = Stanza::new("message".to_string(), None);
		st.tag("body".to_string(), None).unwrap().borrow_mut().text("Hello World!".to_string());
		st.up();

		let root_derefd = st.try_deref();
		assert!(root_derefd.is_some());
		let root_derefd = root_derefd.unwrap();
		assert!(tree::ElementPtr::ptr_eq(&st.root, &root_derefd));
	}

	#[test]
	fn stanza_reset_moves_cursor() {
		let mut st = Stanza::new("iq".to_string(), None);
		st.tag("query".to_string(), None);
		st.tag("extra".to_string(), None);
		st.reset();
		st.tag("error".to_string(), None);

		assert_eq!(st.root().len(), 2);
	}
}
