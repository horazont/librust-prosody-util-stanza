use std::fmt;
use std::rc::Rc;
use std::collections::HashMap;
use std::cell::{RefCell, Ref, RefMut};
use std::ops::{Deref, DerefMut};

#[derive(PartialEq, Debug)]
pub enum InsertError {
	LoopDetected,
}

impl fmt::Display for InsertError {
	fn fmt<'a>(&self, f: &'a mut fmt::Formatter) -> fmt::Result {
		match self {
			Self::LoopDetected => write!(f, "loop detected"),
		}
	}
}

pub enum MapElementsError<T> {
	Structural(InsertError),
	External(T),
}

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

	pub fn deep_clone(&self) -> ElementPtr {
		ElementPtr::wrap(self.borrow().deep_clone())
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
	children: Children,

	/// This is set to true if and only if the element has no parent, nowhere.
	/// By default, this is set to true.
	/// It is set to false by Children::push.
	is_root: bool,
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

	pub fn deep_clone(&self) -> Node {
		match self {
			Node::Text(s) => Node::Text(s.clone()),
			Node::Element(el) => Node::Element(el.deep_clone()),
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

	fn push_element(&mut self, el: ElementPtr) -> Option<InsertError> {
		{
			let mut el = el.borrow_mut();
			el.is_root = false;
		}
		let index = self.all.len();
		self.all.push(Node::Element(el));
		self.element_indices.push(index);
		None
	}

	fn push_other(&mut self, n: Node) -> Option<InsertError> {
		self.all.push(n);
		None
	}

	pub fn push(&mut self, n: Node) -> Option<InsertError> {
		match n {
			Node::Element(el) => self.push_element(el),
			_ => self.push_other(n),
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
			is_root: true,
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

	pub fn deep_clone(&self) -> Element {
		let mut result = Element::new_with_attr(
			self.localname.clone(),
			Some(self.attr.clone()),
		);
		for child_node in &self.children.all {
			result.children.push(child_node.deep_clone());
		}
		result
	}

	fn may_insert<'a>(&self, el: &'a ElementPtr) -> bool {
		if std::ptr::eq(el.as_ptr(), self as *const Element) {
			return false;
		}
		if !el.borrow().is_root {
			return false;
		}
		true
	}

	pub fn map_elements<F, T>(&mut self, f: F) -> Option<MapElementsError<T>>
		where F: Fn(ElementPtr) -> Result<Option<ElementPtr>, T>
	{
		let mut new_children = Children::new();
		for node in &self.children.all {
			match node {
				Node::Text(_) => new_children.push(node.clone()),
				Node::Element(el) => {
					match f(el.clone()) {
						Err(e) => return Some(MapElementsError::External(e)),
						Ok(None) => continue,
						Ok(Some(new_el)) => {
							// we need to do the usual loop-detection dance
							// here
							// Note: if the ptr is equal, we can re-insert
							// here no matter what any other check says.
							if !ElementPtr::ptr_eq(&new_el, &el) && !self.may_insert(&new_el) {
								return Some(MapElementsError::Structural(InsertError::LoopDetected));
							}
							new_children.push(Node::Element(new_el))
						}
					}
				}
			};
		}
		self.children = new_children;
		None
	}

	pub fn is_root(&self) -> bool {
		self.is_root
	}

	pub fn element_view(&self) -> ElementView {
		self.children.element_view()
	}

	pub fn push(&mut self, n: Node) -> Option<InsertError> {
		if let Node::Element(el) = &n {
			if !self.may_insert(&el) {
				return Some(InsertError::LoopDetected);
			}
		}
		self.children.push(n)
	}
}

// gives us all the goodies rust slices have, including stuff like len(),
// is_empty() etc.
impl Deref for Element {
	type Target = [Node];

	fn deref(&self) -> &[Node] {
		&self.children
	}
}

impl DerefMut for Element {
	fn deref_mut(&mut self) -> &mut [Node] {
		&mut self.children
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

	fn prep_message() -> ElementPtr {
		let mut msg = Element::new("message".to_string());
		msg.tag("body".to_string(), None).borrow_mut().text("Hello World!".to_string());
		msg.tag("delay".to_string(), None);
		ElementPtr::wrap(msg)
	}

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
	fn children_push_element_clears_root_bit() {
		let mut c = Children::new();
		c.push(Node::wrap_element(Element::new("el".to_string())));

		assert!(c.len() == 1);
		assert!(!c.is_empty());

		assert!(!c[0].as_element_ptr().unwrap().borrow().is_root());
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
	fn element_push_does_not_allow_cycles() {
		let mut el = Element::new("message".to_string());
		el.tag("foo".to_string(), None);
		assert_eq!(el.push(el[0].clone()), Some(InsertError::LoopDetected));
		assert_eq!(el.len(), 1);
	}

	#[test]
	fn element_push_does_not_allow_self_insertion() {
		let el_ptr = ElementPtr::wrap(Element::new("message".to_string()));
		let n = Node::Element(el_ptr.clone());
		assert_eq!(el_ptr.borrow_mut().push(n), Some(InsertError::LoopDetected));
		assert_eq!(el_ptr.borrow().len(), 0);
	}

	#[test]
	fn element_push_allows_adding_freestanding_element() {
		let mut msg = Element::new("message".to_string());
		let body = ElementPtr::wrap(Element::new("message".to_string()));
		body.borrow_mut().text("foobar".to_string());
		msg.push(Node::Element(body.clone()));
		assert_eq!(msg.len(), 1);
		assert!(ElementPtr::ptr_eq(&body, &msg[0].as_element_ptr().unwrap()));
	}

	#[test]
	fn element_map_elements_allows_identity_transform() {
		let el_ptr = prep_message();
		assert_eq!(el_ptr.borrow().len(), 2);
		let map_result = el_ptr.borrow_mut().map_elements::<_, ()>(|el| {
			Ok(Some(el))
		});
		assert!(map_result.is_none());
		assert_eq!(el_ptr.borrow().len(), 2);
	}

	#[test]
	fn element_map_elements_rejects_self_insertion() {
		let el_ptr = prep_message();
		assert_eq!(el_ptr.borrow().len(), 2);
		let map_result = el_ptr.borrow_mut().map_elements::<_, ()>(|_| {
			Ok(Some(el_ptr.clone()))
		});
		assert!(match map_result {
			Some(MapElementsError::Structural(InsertError::LoopDetected)) => true,
			_ => false,
		})
	}

	#[test]
	fn element_map_elements_rejects_insertion_of_parent_at_child() {
		let el_ptr = prep_message();
		assert_eq!(el_ptr.borrow().len(), 2);
		let map_result = el_ptr.borrow_mut().map_elements::<_, ()>(|_| {
			Ok(Some(el_ptr.clone()))
		});
		assert!(match map_result {
			Some(MapElementsError::Structural(InsertError::LoopDetected)) => true,
			_ => false,
		})
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
