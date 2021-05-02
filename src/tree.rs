use std::fmt;
use std::rc::Rc;
use std::collections::HashMap;
use std::cell::{RefCell, Ref, RefMut};
use std::ops::{Deref, DerefMut};

#[derive(Clone)]
pub struct ElementPtr(Rc<RefCell<Element>>);

impl ElementPtr {
	pub fn wrap(el: Element) -> ElementPtr {
		ElementPtr(Rc::new(RefCell::new(el)))
	}

	pub fn borrow<'a>(&'a self) -> Ref<'a, Element> {
		(*self.0).borrow()
	}

	pub fn borrow_mut<'a>(&'a self) -> RefMut<'a, Element> {
		(*self.0).borrow_mut()
	}

	pub fn ptr_eq(this: &Self, other: &Self) -> bool {
		Rc::ptr_eq(&this.0, &other.0)
	}
}

impl Deref for ElementPtr {
	type Target = RefCell<Element>;

	fn deref(&self) -> &Self::Target {
		&*self.0
	}
}

#[derive(Clone)]
pub enum Node {
	Text(String),
	Element(ElementPtr),
}

pub struct Children {
	all: Vec<Node>,
	element_indices: Vec<usize>,
}

pub struct ElementView<'a> {
	all: &'a Vec<Node>,
	indices: &'a Vec<usize>,
}

pub struct Element {
	pub localname: String,
	pub attr: HashMap<String, String>,
	pub children: Children,
}

impl Node {
	pub fn wrap_element(el: Element) -> Node {
		Node::Element(ElementPtr::wrap(el))
	}

	pub fn wrap_text(t: String) -> Node {
		Node::Text(t)
	}

	pub fn as_element_ptr<'a>(&'a self) -> Option<ElementPtr> {
		if let Node::Element(el) = self {
			Some(el.clone())
		} else {
			None
		}
	}

	pub fn as_text<'a>(&'a self) -> Option<&'a String> {
		if let Node::Text(s) = self {
			Some(s)
		} else {
			None
		}
	}
}

impl fmt::Debug for Node {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Node::Text(s) => write!(f, "Node::Text({:#?})", s),
			Node::Element(el) => write!(f, "Node::{:#?}", *el.deref()),
		}
	}
}

impl Children {
	pub fn new() -> Children {
		Children{
			all: Vec::new(),
			element_indices: Vec::new(),
		}
	}

	pub fn push(&mut self, n: Node) {
		let is_element = n.as_element_ptr().is_some();
		let index = self.all.len();
		self.all.push(n);
		if is_element {
			self.element_indices.push(index);
		}
	}

	pub fn element_view<'a>(&'a self) -> ElementView<'a> {
		ElementView{all: &self.all, indices: &self.element_indices}
	}
}

// gives us all the goodies rust slices have, including stuff like len(),
// is_empty() etc.
impl Deref for Children {
	type Target = [Node];

	fn deref(&self) -> &[Node] {
		&self.all
	}
}

impl DerefMut for Children {
	fn deref_mut(&mut self) -> &mut [Node] {
		&mut self.all
	}
}

impl fmt::Debug for Children {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Debug::fmt(&self.all, f)
	}
}

// we cannot implement Deref for ElementView, because it is not contiguous
impl ElementView<'_> {
	#[inline]
	pub fn len(&self) -> usize {
		self.indices.len()
	}

	#[inline]
	pub fn is_empty(&self) -> bool {
		self.indices.is_empty()
	}

	pub fn get_index(&self, i: usize) -> Option<usize> {
		Some(*self.indices.get(i)?)
	}
}

impl Element {
	pub fn new(name: String) -> Element {
		Element::new_with_attr(name, None)
	}

	pub fn new_with_attr(name: String, attr: Option<HashMap<String, String>>) -> Element {
		Element{
			localname: name,
			attr: match attr {
				Some(attr) => attr,
				None => HashMap::new(),
			},
			children: Children::new(),
		}
	}

	pub fn tag<'a>(&'a mut self, name: String, attr: Option<HashMap<String, String>>) -> ElementPtr {
		let new_child = Node::wrap_element(Element::new_with_attr(name, attr));
		let index = self.children.len();
		self.children.push(new_child);
		self.children[index].as_element_ptr().unwrap()
	}

	pub fn text(&mut self, text: String) {
		let new_child = Node::wrap_text(text);
		self.children.push(new_child);
	}
}

impl fmt::Debug for Element {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "Element{{ localname = {:?}, attr = {:#?}, children = {:#?} }}", self.localname, self.attr, self.children)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn node_as_text_on_text() {
		assert_eq!(Node::Text("foobar".to_string()).as_text(), Some(&"foobar".to_string()));
	}

	#[test]
	fn node_as_element_on_text() {
		assert!(Node::Text("foobar".to_string()).as_element_ptr().is_none())
	}

	#[test]
	fn children_new() {
		let c = Children::new();
		assert!(c.len() == 0);
		assert!(c.is_empty());
	}

	#[test]
	fn children_push_text() {
		let mut c = Children::new();
		c.push(Node::Text("foobar".to_string()));

		assert!(c.len() == 1);
		assert!(!c.is_empty());

		assert_eq!(c[0].as_text(), Some(&"foobar".to_string()));
	}

	#[test]
	fn children_push_element() {
		let mut c = Children::new();
		c.push(Node::wrap_element(Element::new("el".to_string())));

		assert!(c.len() == 1);
		assert!(!c.is_empty());

		assert_eq!(c[0].as_element_ptr().unwrap().borrow().localname, "el");
	}

	#[test]
	fn children_element_view_empty_for_texts() {
		let mut c = Children::new();
		c.push(Node::Text("foo".to_string()));
		c.push(Node::Text("bar".to_string()));
		c.push(Node::Text("baz".to_string()));

		assert!(c.len() == 3);

		let view = c.element_view();
		assert!(view.len() == 0);
		assert!(view.is_empty());
	}

	#[test]
	fn children_element_view_mixed() {
		let mut c = Children::new();
		c.push(Node::wrap_element(Element::new("el1".to_string())));
		c.push(Node::Text("foo".to_string()));
		c.push(Node::wrap_element(Element::new("el2".to_string())));
		c.push(Node::Text("bar".to_string()));
		c.push(Node::wrap_element(Element::new("el3".to_string())));
		c.push(Node::Text("baz".to_string()));

		assert!(c.len() == 6);

		let view = c.element_view();
		assert_eq!(view.len(), 3);

		assert_eq!(view.get_index(0), Some(0));
		assert_eq!(view.get_index(1), Some(2));
		assert_eq!(view.get_index(2), Some(4));
	}

	#[test]
	fn element_new() {
		let el = Element::new("message".to_string());
		assert_eq!(el.localname, "message");
		assert!(el.children.is_empty());
		assert!(el.attr.is_empty());
	}

	#[test]
	fn element_tag() {
		let mut el = Element::new("message".to_string());
		{
			let body_ptr = el.tag("body".to_string(), None);
			let body = body_ptr.borrow();
			assert_eq!(body.localname, "body");
			assert!(body.attr.is_empty());
			assert!(body.children.is_empty());
		}
		assert_eq!(el.children.len(), 1);
		assert_eq!(el.children.element_view().len(), 1);
		assert_eq!(el.children[0].as_element_ptr().unwrap().borrow().localname, "body");
	}

	#[test]
	fn element_text() {
		let mut el = Element::new("message".to_string());
		let body_ptr = el.tag("body".to_string(), None);
		let mut body = body_ptr.borrow_mut();
		body.text("Hello World!".to_string());
		assert_eq!(body.children.len(), 1);
		assert_eq!(body.children.element_view().len(), 0);
		assert_eq!(body.children[0].as_text().unwrap(), "Hello World!");
	}
}
