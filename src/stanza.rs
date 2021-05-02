use std::fmt;
use std::sync::Arc;
use std::cell::{RefCell, Ref, RefMut};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut, Index};

pub struct Children {
	all: Vec<Node>,
	elements: Vec<usize>,
}

pub struct ElementView<'a> {
	all: &'a Vec<Node>,
	elements: &'a Vec<usize>,
}

pub struct Element {
	pub localname: String,
	pub attr: HashMap<String, String>,
	pub children: Children,
}

pub enum Node {
	Text(String),
	Element(Element),
}

#[derive(Clone)]
pub struct StanzaPath(Arc<RefCell<Node>>, Vec<usize>);

impl fmt::Debug for Node {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Node::Text(s) => write!(f, "Node::Text({:#?})", s),
			Node::Element(el) => write!(f, "Node::{:#?}", el),
		}
	}
}

impl fmt::Debug for Children {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Debug::fmt(&self.all, f)
	}
}

impl fmt::Debug for StanzaPath {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self.deref() {
			Some(noderef) => write!(f, "StanzaPath->{:#?}", noderef),
			None => write!(f, "StanzaPath-><invalid>"),
		}
	}
}

impl Element {
	pub fn new(name: String) -> Element {
		Element::new_with_attr(name, HashMap::new())
	}

	pub fn new_with_attr(name: String, attr: HashMap<String, String>) -> Element {
		Element{
			localname: name,
			attr: attr,
			children: Children::new(),
		}
	}

	pub fn new_node(name: String) -> Node {
		Element::new_node_with_attr(name, HashMap::new())
	}

	pub fn new_node_with_attr(name: String, attr: HashMap<String, String>) -> Node {
		Node::Element(Element::new_with_attr(name, attr))
	}

	pub fn tag(&mut self, name: String, attr: HashMap<String, String>) -> &mut Element {
		let new_el = Element::new_node_with_attr(name, attr);
		let index = self.children.len();
		self.children.push(new_el);
		match self.children.get_mut(index).unwrap() {
			Node::Element(el) => el,
			_ => panic!("type assertion failed"),
		}
	}

	pub fn text(&mut self, data: String) -> () {
		self.children.push(Node::Text(data))
	}

	pub fn get_child_index(&self, name: Option<String>, xmlns: Option<String>) -> Option<usize> {
		let (has_name, name) = match name {
			Some(n) => (true, n),
			None => (false, "".to_string()),
		};
		let (has_xmlns, xmlns) = match xmlns {
			Some(ns) => (true, ns),
			None => match self.attr.get("xmlns") {
				Some(ns) => (true, ns),
				None => (false, ""),
			},
		};

		for child_index in self.children.elements.iter() {
			let child_node = &self.children[*child_index];
			let child_el = match child {
				Node::Element(el) => el,
				_ => continue,
			};

			if (has_name && name != child_el.localname) {
				continue;
			}


			// return element if either is true:
			// - xmlns arg is None && child has no xmlns set
			// - xmlns arg is None && child xmlns matches parent xmlns
			// - xmlns arg is Some && child xmlns matches arg
			if has_xmlns {
				match child_el.attr.get("xmlns")
			}
		}
	}
}

impl fmt::Debug for Element {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "Element{{ localname = {:?}, attr = {:#?}, children = {:#?} }}", self.localname, self.attr, self.children)
	}
}

impl Children {
	pub fn new() -> Children {
		Children{
			all: Vec::new(),
			elements: Vec::new(),
		}
	}

	pub fn element_view<'a>(&'a self) -> ElementView<'a> {
		ElementView{all: &self.all, elements: &self.elements}
	}

	pub fn push(&mut self, node: Node) {
		let index = self.all.len();
		if let Node::Element(_) = &node {
			self.elements.push(index);
		}
		self.all.push(node);
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

// we cannot implement Deref for ElementView, because it is not contiguous
impl ElementView<'_> {
	pub fn len(&self) -> usize {
		self.elements.len()
	}

	fn deref_element<'a>(node: &'a Node) -> &'a Element {
		if let Node::Element(el) = node {
			el
		} else {
			panic!("element offset refers to non-element")
		}
	}

	#[inline]
	pub fn get<'a>(&'a self, i: usize) -> Option<&'a Element> {
		let offset = *self.elements.get(i)?;
		Some(ElementView::deref_element(&self.all[offset]))
	}

	pub fn get_index(&self, i: usize) -> Option<usize> {
		Some(*self.elements.get(i)?)
	}
}

impl Index<usize> for ElementView<'_> {
	type Output = Element;

	#[inline]
	fn index(&self, i: usize) -> &Self::Output {
		let offset = self.elements[i];
		if let Node::Element(el) = &self.all[offset] {
			el
		} else {
			panic!("element offset refers to non-element")
		}
	}
}

impl StanzaPath {
	pub fn wrap(el: Element) -> StanzaPath {
		StanzaPath(
			Arc::new(RefCell::new(Node::Element(el))),
			Vec::new(),
		)
	}

	pub fn deref<'a>(&'a self) -> Option<Ref<'a, Node>> {
		let mut curr = self.0.borrow();
		for index in self.1.iter() {
			match &*curr {
				Node::Element(el) => {
					if *index >= el.children.len() {
						return None;
					}
				},
				_ => return None,
			}
			curr = Ref::map(curr, |n| {
				match n {
					Node::Element(el) => &el.children[*index],
					_ => panic!("oh no"),
				}
			})
		}
		Some(curr)
	}

	pub fn deref_mut<'a>(&'a mut self) -> Option<RefMut<'a, Node>> {
		let mut curr = self.0.borrow_mut();
		for index in self.1.iter() {
			match &*curr {
				Node::Element(el) => {
					if *index >= el.children.len() {
						return None;
					}
				},
				_ => return None,
			}
			curr = RefMut::map(curr, |n| {
				match n {
					Node::Element(el) => &mut el.children[*index],
					_ => panic!("oh no"),
				}
			})
		}
		Some(curr)
	}

	pub fn tag<'a>(&'a mut self, name: String, attr: HashMap<String, String>) -> Option<StanzaPath> {
		let mut new_path = self.1.clone();
		{
			let mut el = as_element_mut(self.deref_mut()?)?;
			el.tag(name, attr);
			new_path.push(el.children.len() - 1);
		}
		Some(StanzaPath(
			self.0.clone(),
			new_path,
		))
	}

	pub fn text(&mut self, data: String) -> Option<StanzaPath> {
		{
			let mut el = as_element_mut(self.deref_mut()?)?;
			el.text(data);
		}
		Some(self.clone())
	}

	pub fn up(&self) -> Option<StanzaPath> {
		if self.1.len() == 0 {
			None
		} else {
			let mut new_path = self.1.clone();
			new_path.pop();
			Some(StanzaPath(
				self.0.clone(),
				new_path,
			))
		}
	}

	pub fn reset(&self) -> StanzaPath {
		StanzaPath(
			self.0.clone(),
			Vec::new(),
		)
	}

	pub fn down(&self, i: usize) -> StanzaPath {
		let mut new_path = self.1.clone();
		new_path.push(i);
		StanzaPath(
			self.0.clone(),
			new_path,
		)
	}

	pub fn deref_as_element<'a>(&'a self) -> Option<Ref<'a, Element>> {
		as_element(self.deref()?)
	}

	pub fn deref_as_element_mut<'a>(&'a mut self) -> Option<RefMut<'a, Element>> {
		as_element_mut(self.deref_mut()?)
	}
}

fn as_element<'a>(r: Ref<'a, Node>) -> Option<Ref<'a, Element>> {
	match *r {
		Node::Text(_) => return None,
		_ => (),
	};

	let result: Ref<Element> = Ref::map(r, |rr| {
		match rr {
			Node::Element(el) => el,
			_ => panic!("oh no"),
		}
	});
	Some(result)
}

fn as_element_mut<'a>(r: RefMut<'a, Node>) -> Option<RefMut<'a, Element>> {
	match *r {
		Node::Text(_) => return None,
		_ => (),
	};

	let result: RefMut<Element> = RefMut::map(r, |rr| {
		match rr {
			Node::Element(el) => el,
			_ => panic!("oh no"),
		}
	});
	Some(result)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn stanzapath() {
		let st = StanzaPath::wrap(Element::new("message".to_string()));

		let node = st.deref().unwrap();
		assert_eq!(as_element(node).unwrap().localname, "message");
	}

	#[test]
	fn stanzapath_tag_points_at_child() {
		let mut st = StanzaPath::wrap(Element::new("message".to_string()));
		let child = st.tag("body".to_string(), HashMap::new()).unwrap();
		let node = child.deref().unwrap();
		assert_eq!(as_element(node).unwrap().localname, "body");
	}

	#[test]
	fn stanzapath_tag_points_at_child_mut() {
		let mut st = StanzaPath::wrap(Element::new("message".to_string()));
		let mut child = st.tag("body".to_string(), HashMap::new()).unwrap();
		let node = child.deref_mut().unwrap();
		assert_eq!(as_element_mut(node).unwrap().localname, "body");
	}

	#[test]
	fn stanzapath_tag_up() {
		let mut st = StanzaPath::wrap(Element::new("message".to_string()));
		let st = st.tag("body".to_string(), HashMap::new()).unwrap().up().unwrap();
		{
			let node = st.deref().unwrap();
			assert_eq!(as_element(node).unwrap().localname, "message");
		}
		{
			let node = st.deref().unwrap();
			assert_eq!(as_element(node).unwrap().children.len(), 1);
		}
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

		match &c[0] {
			Node::Text(s) => assert_eq!(s, "foobar"),
			_ => assert!(false),
		}
	}

	#[test]
	fn children_push_element() {
		let mut c = Children::new();
		c.push(Element::new_node("el".to_string()));

		assert!(c.len() == 1);
		assert!(!c.is_empty());

		match &c[0] {
			Node::Element(el) => assert_eq!(el.localname, "el"),
			_ => assert!(false),
		}
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
	}

	#[test]
	fn children_element_view_for_elements() {
		let mut c = Children::new();
		c.push(Element::new_node("el1".to_string()));
		c.push(Element::new_node("el2".to_string()));
		c.push(Element::new_node("el3".to_string()));

		assert!(c.len() == 3);

		let view = c.element_view();
		assert!(view.len() == 3);

		for (i, node) in c.iter().enumerate() {
			match &node {
				Node::Element(el) => assert_eq!(el.localname, view[i].localname),
				_ => assert!(false),
			}
		}
	}

	#[test]
	fn children_element_view_mixed() {
		let mut c = Children::new();
		c.push(Element::new_node("el1".to_string()));
		c.push(Node::Text("foo".to_string()));
		c.push(Element::new_node("el2".to_string()));
		c.push(Node::Text("bar".to_string()));
		c.push(Element::new_node("el3".to_string()));
		c.push(Node::Text("baz".to_string()));

		assert!(c.len() == 6);

		let view = c.element_view();
		assert!(view.len() == 3);

		match &c[0] {
			Node::Element(el) => assert_eq!(el.localname, view[0].localname),
			_ => assert!(false),
		}

		match &c[2] {
			Node::Element(el) => assert_eq!(el.localname, view[1].localname),
			_ => assert!(false),
		}

		match &c[4] {
			Node::Element(el) => assert_eq!(el.localname, view[2].localname),
			_ => assert!(false),
		}
	}
}
