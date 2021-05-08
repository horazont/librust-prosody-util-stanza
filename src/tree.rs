use std::fmt;
use std::rc::{Rc, Weak};
use std::collections::HashMap;
use std::cell::{RefCell, Ref, RefMut};
use std::ops::{Deref, DerefMut};
use std::slice::Iter;

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

#[derive(Debug)]
pub enum MapElementsError<T> {
	Structural(InsertError),
	External(T),
}

#[derive(Clone)]
pub struct ElementPtr(Rc<RefCell<Element>>);

#[derive(Clone)]
struct WeakElementPtr(Weak<RefCell<Element>>);

impl ElementPtr {
	pub fn new(name: String) -> ElementPtr {
		ElementPtr::wrap(Element::raw_new(name))
	}

	pub fn new_with_attr(name: String, attr: Option<HashMap<String, String>>) -> ElementPtr {
		ElementPtr::wrap(Element::raw_new_with_attr(name, attr))
	}

	pub fn wrap(el: Element) -> ElementPtr {
		if el.self_ptr.borrow().upgrade().is_some() {
			panic!("attempt to wrap an already wrapped Element");
		}
		let wrapped = ElementPtr(Rc::new(RefCell::new(el)));
		*wrapped.borrow_mut().self_ptr.borrow_mut() = wrapped.downgrade();
		wrapped
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

	fn downgrade(&self) -> WeakElementPtr {
		WeakElementPtr(Rc::downgrade(&self.0))
	}

	pub fn deep_clone(&self) -> ElementPtr {
		ElementPtr::wrap(self.borrow().deep_clone())
	}
}

impl PartialEq for ElementPtr {
	fn eq(&self, other: &ElementPtr) -> bool {
		ElementPtr::ptr_eq(self, other) || *self.borrow() == *other.borrow()
	}
}

impl Deref for ElementPtr {
	type Target = RefCell<Element>;

	fn deref(&self) -> &Self::Target {
		&*self.0
	}
}

impl WeakElementPtr {
	pub fn new() -> WeakElementPtr {
		WeakElementPtr(Weak::new())
	}

	pub fn upgrade(&self) -> Option<ElementPtr> {
		let raw = self.0.upgrade()?;
		Some(ElementPtr(raw))
	}

	pub fn is_valid(&self) -> bool {
		self.0.strong_count() > 0
	}
}

#[derive(Clone, PartialEq)]
pub enum Node {
	Text(String),
	Element(ElementPtr),
}

#[derive(PartialEq)]
pub struct Children {
	all: Vec<Node>,
	element_indices: Vec<usize>,
}

pub struct ElementView<'a> {
	all: &'a Vec<Node>,
	indices: &'a Vec<usize>,
}

pub struct ChildElementIterator<'a> {
	all: &'a Vec<Node>,
	indices: Iter<'a, usize>,
}

impl<'a> Iterator for ChildElementIterator<'a> {
	type Item = ElementPtr;

	fn next(&mut self) -> Option<Self::Item> {
		let index = *self.indices.next()?;
		Some(self.all[index].as_element_ptr().unwrap())
	}
}

pub struct Element {
	pub localname: String,
	pub attr: HashMap<String, String>,
	pub namespaces: HashMap<String, String>,
	children: Children,
	// using cells here because those don’t actually change anything about
	// the logical Element ... it would otherwise require to have the elements
	// be mutable to be inserted in a subtree.
	// using a refcell because Weak is not copyable.
	parent: RefCell<WeakElementPtr>,
	self_ptr: RefCell<WeakElementPtr>,
}

impl Node {
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
			Node::Element(el) => write!(f, "Node::{:#?}", *el.borrow()),
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

	fn push_element(&mut self, el: ElementPtr) {
		let index = self.all.len();
		self.all.push(Node::Element(el));
		self.element_indices.push(index);
	}

	fn push_other(&mut self, n: Node) {
		self.all.push(n);
	}

	pub fn push(&mut self, n: Node) {
		match n {
			Node::Element(el) => self.push_element(el),
			_ => self.push_other(n),
		}
	}

	pub fn element_view<'a>(&'a self) -> ElementView<'a> {
		ElementView{all: &self.all, indices: &self.element_indices}
	}

	pub fn iter_children<'a>(&'a self) -> ChildElementIterator<'a> {
		ChildElementIterator{all: &self.all, indices: self.element_indices.iter()}
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
		f.debug_list().entries(&self.all).finish()
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

	pub fn get(&self, i: usize) -> Option<ElementPtr> {
		let index = self.get_index(i)?;
		self.all.get(index)?.as_element_ptr()
	}
}

impl Element {
	fn raw_new(name: String) -> Element {
		Element::raw_new_with_attr(name, None)
	}

	fn raw_new_with_attr(name: String, attr: Option<HashMap<String, String>>) -> Element {
		Element{
			localname: name,
			attr: match attr {
				Some(attr) => attr,
				None => HashMap::new(),
			},
			namespaces: HashMap::new(),
			children: Children::new(),
			parent: RefCell::new(WeakElementPtr::new()),
			self_ptr: RefCell::new(WeakElementPtr::new()),
		}
	}

	fn self_ptr(&self) -> WeakElementPtr {
		debug_assert!(self.self_ptr.borrow().upgrade().is_some());
		self.self_ptr.borrow().clone()
	}

	pub fn tag<'a>(&'a mut self, name: String, attr: Option<HashMap<String, String>>) -> ElementPtr {
		let result_ptr = ElementPtr::new_with_attr(name, attr);
		let new_node = Node::Element(result_ptr.clone());
		self.push(new_node).and_then(|err| -> Option<()> {
			panic!("failed to insert fresh node: {:?}", err);
		});
		result_ptr
	}

	pub fn text(&mut self, text: String) {
		let new_child = Node::wrap_text(text);
		self.children.push(new_child);
	}

	pub fn deep_clone(&self) -> Element {
		let mut result = Element::raw_new_with_attr(
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
		if el.borrow().parent.borrow().is_valid() {
			// do not allow insertion of elements which have a parent.
			return false;
		}
		// finally check that the element to be inserted is not our (possibly
		// indirect) parent
		let mut parent_opt = self.parent();
		while parent_opt.is_some() {
			let parent_ptr = parent_opt.unwrap();
			if ElementPtr::ptr_eq(&el, &parent_ptr) {
				return false;
			}
			parent_opt = parent_ptr.borrow().parent();
		}
		true
	}

	fn parent(&self) -> Option<ElementPtr> {
		self.parent.borrow().upgrade()
	}

	fn clear_parent(&self) {
		*self.parent.borrow_mut() = WeakElementPtr::new();
	}

	pub fn map_elements<F, T>(&mut self, f: F) -> Result<(), MapElementsError<T>>
		where F: Fn(ElementPtr) -> Result<Option<ElementPtr>, T>
	{
		let mut new_children = Children::new();
		for node in &self.children.all {
			match node {
				Node::Text(_) => new_children.push(node.clone()),
				Node::Element(el) => {
					match f(el.clone()) {
						Err(e) => return Err(MapElementsError::External(e)),
						Ok(None) => {
							// clear parent reference to orphan element
							el.borrow().clear_parent();
							continue
						},
						Ok(Some(new_el)) => {
							// we need to do the usual loop-detection dance
							// here
							// Note: if the ptr is equal, we can re-insert
							// here no matter what any other check says.
							if !ElementPtr::ptr_eq(&new_el, &el) && !self.may_insert(&new_el) {
								return Err(MapElementsError::Structural(InsertError::LoopDetected));
							}
							new_children.push(Node::Element(new_el))
						}
					}
				}
			};
		}
		self.children = new_children;
		Ok(())
	}

	pub fn iter_children<'a>(&'a self) -> ChildElementIterator {
		self.children.iter_children()
	}

	pub fn element_view(&self) -> ElementView {
		self.children.element_view()
	}

	pub fn push(&mut self, n: Node) -> Option<InsertError> {
		if let Node::Element(el) = &n {
			if !self.may_insert(&el) {
				return Some(InsertError::LoopDetected);
			}
			*el.borrow_mut().parent.borrow_mut() = self.self_ptr();
		}
		self.children.push(n);
		None
	}

	pub fn get_text(&self) -> Option<String> {
		if self.children.element_indices.len() > 0 {
			return None
		}

		// we can now assume that all the nodes are actually texts
		// let’s be super fancy here
		let mut strs = Vec::<&str>::with_capacity(self.len());
		for node in &self.children.all {
			strs.push(node.as_text().unwrap().as_str());
		}
		Some(strs.concat())
	}
}

impl PartialEq for Element {
	// need a custom implementation because we don’t want to compare the
	// self and parent weak refs
	fn eq(&self, other: &Element) -> bool {
		self.localname == other.localname &&
			self.attr == other.attr &&
			self.children == other.children &&
			self.namespaces == other.namespaces
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
		f.debug_struct("Element")
			.field("localname", &self.localname)
			.field("attr", &self.attr)
			.field("children", &self.children)
			.finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn prep_message() -> ElementPtr {
		let msg_ptr = ElementPtr::new("message".to_string());
		{
			let mut msg = msg_ptr.borrow_mut();
			msg.tag("body".to_string(), None).borrow_mut().text("Hello World!".to_string());
			msg.tag("delay".to_string(), None);
		}
		msg_ptr
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
		c.push(Node::Element(ElementPtr::new("el".to_string())));

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
		c.push(Node::Element(ElementPtr::new("el1".to_string())));
		c.push(Node::Text("foo".to_string()));
		c.push(Node::Element(ElementPtr::new("el2".to_string())));
		c.push(Node::Text("bar".to_string()));
		c.push(Node::Element(ElementPtr::new("el3".to_string())));
		c.push(Node::Text("baz".to_string()));

		assert!(c.len() == 6);

		let view = c.element_view();
		assert_eq!(view.len(), 3);

		assert_eq!(view.get_index(0), Some(0));
		assert_eq!(view.get_index(1), Some(2));
		assert_eq!(view.get_index(2), Some(4));
	}

	#[test]
	fn elementptr_new_sets_self_ptr() {
		let el_ptr = ElementPtr::new("message".to_string());
		let upgraded = el_ptr.borrow().self_ptr.borrow().upgrade();
		assert!(upgraded.is_some());
		assert!(ElementPtr::ptr_eq(&el_ptr, &upgraded.unwrap()));
	}

	#[test]
	fn element_new() {
		let el = Element::raw_new("message".to_string());
		assert_eq!(el.localname, "message");
		assert!(el.children.is_empty());
		assert!(el.attr.is_empty());
		assert!(el.self_ptr.borrow().upgrade().is_none());
		assert!(el.parent.borrow().upgrade().is_none());
	}

	#[test]
	fn element_tag() {
		let el_ptr = ElementPtr::new("message".to_string());
		{
			let body_ptr = el_ptr.borrow_mut().tag("body".to_string(), None);
			let body = body_ptr.borrow();
			assert_eq!(body.localname, "body");
			assert!(body.attr.is_empty());
			assert!(body.children.is_empty());
		}
		assert_eq!(el_ptr.borrow().len(), 1);
		assert_eq!(el_ptr.borrow().element_view().len(), 1);
		assert_eq!(el_ptr.borrow()[0].as_element_ptr().unwrap().borrow().localname, "body");
	}

	#[test]
	fn element_push_rejects_child_insertion() {
		let el = ElementPtr::new("message".to_string());
		el.borrow_mut().tag("foo".to_string(), None);
		let el_ptr2 = el.borrow()[0].clone();
		assert_eq!(el.borrow_mut().push(el_ptr2), Some(InsertError::LoopDetected));
		assert_eq!(el.borrow().len(), 1);
	}

	#[test]
	fn element_push_rejects_self_insertion() {
		let el_ptr = ElementPtr::new("message".to_string());
		let n = Node::Element(el_ptr.clone());
		assert_eq!(el_ptr.borrow_mut().push(n), Some(InsertError::LoopDetected));
		assert_eq!(el_ptr.borrow().len(), 0);
	}

	#[test]
	fn element_push_rejects_root_insertion_at_child() {
		let root = ElementPtr::new("root".to_string());
		let child = root.borrow_mut().tag("child".to_string(), None);
		let push_result = child.borrow_mut().push(Node::Element(root.clone()));
		assert_eq!(push_result, Some(InsertError::LoopDetected));
		assert_eq!(child.borrow().len(), 0);
	}

	#[test]
	fn element_push_allows_adding_freestanding_element() {
		let msg = ElementPtr::new("message".to_string());
		let body = ElementPtr::new("body".to_string());
		body.borrow_mut().text("foobar".to_string());
		msg.borrow_mut().push(Node::Element(body.clone()));
		assert_eq!(msg.borrow().len(), 1);
		assert!(ElementPtr::ptr_eq(&body, &msg.borrow()[0].as_element_ptr().unwrap()));
	}

	#[test]
	fn element_map_elements_allows_identity_transform() {
		let el_ptr = prep_message();
		assert_eq!(el_ptr.borrow().len(), 2);
		el_ptr.borrow_mut().map_elements::<_, ()>(|el| {
			Ok(Some(el))
		}).unwrap();
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
			Err(MapElementsError::Structural(InsertError::LoopDetected)) => true,
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
			Err(MapElementsError::Structural(InsertError::LoopDetected)) => true,
			_ => false,
		})
	}

	#[test]
	fn element_map_elements_clear() {
		let el_ptr = prep_message();
		assert_eq!(el_ptr.borrow().len(), 2);
		el_ptr.borrow_mut().map_elements::<_, ()>(|_| {
			Ok(None)
		}).unwrap();
		assert_eq!(el_ptr.borrow().len(), 0);
	}

	#[test]
	fn element_map_elements_removed_elements_are_orphaned() {
		let el_ptr = prep_message();
		assert_eq!(el_ptr.borrow().len(), 2);
		let child_ptr = el_ptr.borrow()[0].as_element_ptr().unwrap();
		el_ptr.borrow_mut().map_elements::<_, ()>(|_| {
			Ok(None)
		}).unwrap();
		assert_eq!(el_ptr.borrow().len(), 0);
		assert!(child_ptr.borrow().parent().is_none());
	}

	#[test]
	fn element_map_elements_removed_elements_can_be_reinserted() {
		let el_ptr = prep_message();
		assert_eq!(el_ptr.borrow().len(), 2);
		let child_ptr = el_ptr.borrow()[0].as_element_ptr().unwrap();
		el_ptr.borrow_mut().map_elements::<_, ()>(|_| {
			Ok(None)
		}).unwrap();
		assert_eq!(el_ptr.borrow().len(), 0);
		assert!(el_ptr.borrow_mut().push(Node::Element(child_ptr)).is_none());
	}

	#[test]
	fn element_text() {
		let el = ElementPtr::new("message".to_string());
		let body_ptr = el.borrow_mut().tag("body".to_string(), None);
		let mut body = body_ptr.borrow_mut();
		body.text("Hello World!".to_string());
		assert_eq!(body.children.len(), 1);
		assert_eq!(body.children.element_view().len(), 0);
		assert_eq!(body.children[0].as_text().unwrap(), "Hello World!");
	}
}
