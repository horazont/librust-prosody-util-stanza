use std::fmt;
use std::rc::{Rc, Weak};
use std::collections::HashMap;
use std::cell::{RefCell, Ref, RefMut};
use std::ops::{Deref, DerefMut};
use std::slice::Iter;
use std::error::Error;

use rxml::CData;

use super::attrstr::AttrName;

#[derive(PartialEq, Debug)]
pub enum InsertError {
	NodeHasParent,
	NodeIsSelf,
	LoopDetected,
	Protected,
}

#[derive(PartialEq, Debug)]
pub enum ProtectError {
	NodeHasParent,
}

impl fmt::Display for InsertError {
	fn fmt<'a>(&self, f: &'a mut fmt::Formatter) -> fmt::Result {
		match self {
			Self::NodeHasParent => f.write_str("the node to insert has a parent already"),
			Self::NodeIsSelf=> f.write_str("the node to insert is the same as the parent node"),
			Self::LoopDetected => f.write_str("inserting the node would create a loop"),
			Self::Protected => f.write_str("parent node is protected"),
		}
	}
}

impl Error for InsertError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

impl fmt::Display for ProtectError {
	fn fmt<'a>(&self, f: &'a mut fmt::Formatter) -> fmt::Result {
		match self {
			Self::NodeHasParent => f.write_str("the node to freeze has a parent already"),
		}
	}
}

impl Error for ProtectError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

#[derive(PartialEq, Debug)]
pub enum MapElementsError<T> {
	Structural(InsertError),
	External(T),
}

#[derive(Clone)]
pub struct ElementPtr(Rc<RefCell<Element>>);

#[derive(Clone)]
struct WeakElementPtr(Weak<RefCell<Element>>);

impl ElementPtr {
	pub fn new(nsuri: Option<Rc<CData>>, name: rxml::Name) -> ElementPtr {
		ElementPtr::wrap(Element::raw_new(nsuri, name))
	}

	pub fn new_with_attr(nsuri: Option<Rc<CData>>, name: rxml::Name, attr: Option<HashMap<AttrName, String>>) -> ElementPtr {
		ElementPtr::wrap(Element::raw_new_with_attr(nsuri, name, attr))
	}

	pub fn wrap(el: Element) -> ElementPtr {
		let wrapped = ElementPtr(Rc::new(RefCell::new(el)));
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
	pub nsuri: Option<Rc<CData>>,
	pub localname: rxml::Name,
	pub attr: HashMap<AttrName, String>,
	// pub namespaces: HashMap<String, String>,
	children: Children,
	protected: bool,
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
	fn raw_new(nsuri: Option<Rc<CData>>, name: rxml::Name) -> Element {
		Element::raw_new_with_attr(nsuri, name, None)
	}

	fn raw_new_with_attr(nsuri: Option<Rc<CData>>, name: rxml::Name, attr: Option<HashMap<AttrName, String>>) -> Element {
		Element{
			nsuri: nsuri,
			localname: name,
			attr: match attr {
				Some(attr) => attr,
				None => HashMap::new(),
			},
			// namespaces: HashMap::new(),
			children: Children::new(),
			protected: false,
		}
	}

	pub fn tag<'a>(&'a mut self, xmlns: Option<Rc<CData>>, name: rxml::Name, attr: Option<HashMap<AttrName, String>>) -> ElementPtr {
		let result_ptr = ElementPtr::new_with_attr(xmlns, name, attr);
		// if this node is protected, we need to inherit that.
		result_ptr.borrow_mut().protected = self.protected;
		let new_node = Node::Element(result_ptr.clone());
		self.push_unchecked(new_node);
		result_ptr
	}

	pub fn text(&mut self, text: String) {
		let new_child = Node::wrap_text(text);
		self.children.push(new_child);
	}

	pub fn deep_clone(&self) -> Element {
		let mut result = Element::raw_new_with_attr(
			self.nsuri.clone(),
			self.localname.clone(),
			Some(self.attr.clone()),
		);
		for child_node in &self.children.all {
			result.children.push(child_node.deep_clone());
		}
		result
	}

	/// Search this element and all child elements for occurences of `el`.
	///
	/// If either `el` is found *or* an element which we cannot borrow because
	/// it is already borrowed mutably, we have to assume that an insert cycle
	/// is going to happen.
	fn deep_reverse_insert_check<'a>(&self) -> Result<(), InsertError> {
		for node in self.iter() {
			if let Some(child) = node.as_element_ptr() {
				let child = match child.try_borrow() {
					Ok(child) => child,
					Err(_) => return Err(InsertError::LoopDetected),
				};
				child.deep_reverse_insert_check()?;
			}
		}
		Ok(())
	}

	fn check_insert<'a>(&self, el: &'a ElementPtr) -> Result<(), InsertError> {
		if std::ptr::eq(el.as_ptr(), self as *const Element) {
			return Err(InsertError::NodeIsSelf);
		}

		if el.borrow().is_protected() {
			// protected guarantees that all child elements are protected, too
			// That implies that `el` is not a parent of this element,
			// otherwise we could not be inserting here (as this element would
			// be protected).
			return Ok(())
		}

		// do a full subtree scan to ensure that we are not creating a loop
		el.borrow().deep_reverse_insert_check()?;
		Ok(())
	}

	pub fn map_elements<F, T>(&mut self, f: F) -> Result<(), MapElementsError<T>>
		where F: Fn(ElementPtr) -> Result<Option<ElementPtr>, T>
	{
		if self.protected {
			return Err(MapElementsError::Structural(InsertError::Protected));
		}
		let mut new_children = Children::new();
		for node in &self.children.all {
			match node {
				Node::Text(_) => new_children.push(node.clone()),
				Node::Element(el) => {
					match f(el.clone()) {
						Err(e) => return Err(MapElementsError::External(e)),
						Ok(None) => continue,
						Ok(Some(new_el)) => {
							// we need to do the usual loop-detection dance
							// here
							// Note: if the ptr is equal, we can re-insert
							// here no matter what any other check says.
							if !ElementPtr::ptr_eq(&new_el, &el) {
								if let Err(e) = self.check_insert(&new_el) {
									return Err(MapElementsError::Structural(e));
								}
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

	fn push_unchecked(&mut self, n: Node) {
		self.children.push(n);
	}

	pub fn push(&mut self, n: Node) -> Result<(), InsertError> {
		if self.protected {
			return Err(InsertError::Protected);
		}
		if let Node::Element(el) = &n {
			self.check_insert(&el)?;
		}
		self.push_unchecked(n);
		Ok(())
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

	#[inline]
	pub fn is_protected(&self) -> bool {
		self.protected
	}

	fn protect_rec(&mut self) -> Result<(), ProtectError> {
		for node in self.iter() {
			match node {
				Node::Text(_) => (),
				Node::Element(e) => {
					e.borrow_mut().protect_rec()?;
				}
			}
		}
		self.protected = true;
		Ok(())
	}

	pub fn protect(&mut self) -> Result<(), ProtectError> {
		self.protect_rec()
	}
}

impl PartialEq for Element {
	// need a custom implementation because we don’t want to compare the
	// protected field
	fn eq(&self, other: &Element) -> bool {
		self.localname == other.localname &&
			self.attr == other.attr &&
			self.children == other.children /* &&
			self.namespaces == other.namespaces */
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
		let mut st = f.debug_struct("Element");
		let mut st = if let Some(nsuri) = self.nsuri.as_ref() {
			st.field("nsuri", &*nsuri)
		} else {
			st.field("nsuri", &self.nsuri)
		};
		st.field("localname", &self.localname)
			.field("attr", &self.attr)
			.field("children", &self.children)
			.finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::convert::TryInto;

	fn prep_message() -> ElementPtr {
		let msg_ptr = ElementPtr::new(None, "message".try_into().unwrap());
		{
			let mut msg = msg_ptr.borrow_mut();
			msg.tag(None, "body".try_into().unwrap(), None).borrow_mut().text("Hello World!".try_into().unwrap());
			msg.tag(None, "delay".try_into().unwrap(), None);
		}
		msg_ptr
	}

	#[test]
	fn node_as_text_on_text() {
		assert_eq!(Node::Text("foobar".try_into().unwrap()).as_text(), Some(&"foobar".try_into().unwrap()));
	}

	#[test]
	fn node_as_element_on_text() {
		assert!(Node::Text("foobar".try_into().unwrap()).as_element_ptr().is_none())
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
		c.push(Node::Text("foobar".try_into().unwrap()));

		assert!(c.len() == 1);
		assert!(!c.is_empty());

		assert_eq!(c[0].as_text(), Some(&"foobar".try_into().unwrap()));
	}

	#[test]
	fn children_push_element() {
		let mut c = Children::new();
		c.push(Node::Element(ElementPtr::new(None, "el".try_into().unwrap())));

		assert!(c.len() == 1);
		assert!(!c.is_empty());

		assert_eq!(c[0].as_element_ptr().unwrap().borrow().localname, "el");
	}

	#[test]
	fn children_element_view_empty_for_texts() {
		let mut c = Children::new();
		c.push(Node::Text("foo".try_into().unwrap()));
		c.push(Node::Text("bar".try_into().unwrap()));
		c.push(Node::Text("baz".try_into().unwrap()));

		assert!(c.len() == 3);

		let view = c.element_view();
		assert!(view.len() == 0);
		assert!(view.is_empty());
	}

	#[test]
	fn children_element_view_mixed() {
		let mut c = Children::new();
		c.push(Node::Element(ElementPtr::new(None, "el1".try_into().unwrap())));
		c.push(Node::Text("foo".try_into().unwrap()));
		c.push(Node::Element(ElementPtr::new(None, "el2".try_into().unwrap())));
		c.push(Node::Text("bar".try_into().unwrap()));
		c.push(Node::Element(ElementPtr::new(None, "el3".try_into().unwrap())));
		c.push(Node::Text("baz".try_into().unwrap()));

		assert!(c.len() == 6);

		let view = c.element_view();
		assert_eq!(view.len(), 3);

		assert_eq!(view.get_index(0), Some(0));
		assert_eq!(view.get_index(1), Some(2));
		assert_eq!(view.get_index(2), Some(4));
	}

	#[test]
	fn element_new() {
		let el = Element::raw_new(None, "message".try_into().unwrap());
		assert_eq!(el.localname, "message");
		assert!(el.children.is_empty());
		assert!(el.attr.is_empty());
	}

	#[test]
	fn element_tag() {
		let el_ptr = ElementPtr::new(None, "message".try_into().unwrap());
		{
			let body_ptr = el_ptr.borrow_mut().tag(None, "body".try_into().unwrap(), None);
			let body = body_ptr.borrow();
			assert_eq!(body.nsuri, None);
			assert_eq!(body.localname, "body");
			assert!(body.attr.is_empty());
			assert!(body.children.is_empty());
		}
		assert_eq!(el_ptr.borrow().len(), 1);
		assert_eq!(el_ptr.borrow().element_view().len(), 1);
		assert_eq!(el_ptr.borrow()[0].as_element_ptr().unwrap().borrow().localname, "body");
	}

	#[test]
	fn element_tag_inherits_nsuri_from_parent() {
		let nsuri = Some(Rc::new("uri:foobar".try_into().unwrap()));
		let el_ptr = ElementPtr::new(nsuri.clone(), "message".try_into().unwrap());
		{
			let body_ptr = el_ptr.borrow_mut().tag(None, "body".try_into().unwrap(), None);
			let body = body_ptr.borrow();
			assert_eq!(body.nsuri, nsuri);
			assert_eq!(body.localname, "body");
			assert!(body.attr.is_empty());
			assert!(body.children.is_empty());
		}
		assert_eq!(el_ptr.borrow().len(), 1);
		assert_eq!(el_ptr.borrow().element_view().len(), 1);
		assert_eq!(el_ptr.borrow()[0].as_element_ptr().unwrap().borrow().localname, "body");
	}

	#[test]
	fn element_push_rejects_self_insertion() {
		let el_ptr = ElementPtr::new(None, "message".try_into().unwrap());
		let n = Node::Element(el_ptr.clone());
		assert_eq!(el_ptr.borrow_mut().push(n), Err(InsertError::NodeIsSelf));
		assert_eq!(el_ptr.borrow().len(), 0);
	}

	#[test]
	fn element_push_rejects_parent_insertion() {
		let root = prep_message();
		let act_on = root.borrow()[1].as_element_ptr().unwrap();
		let n = Node::Element(root.clone());
		assert_eq!(act_on.borrow_mut().push(n), Err(InsertError::LoopDetected));
		assert_eq!(act_on.borrow().len(), 0);
	}

	#[test]
	fn element_push_rejects_root_insertion_at_child() {
		let root = ElementPtr::new(None, "root".try_into().unwrap());
		let child = root.borrow_mut().tag(None, "child".try_into().unwrap(), None);
		let push_result = child.borrow_mut().push(Node::Element(root.clone()));
		assert_eq!(push_result, Err(InsertError::LoopDetected));
		assert_eq!(child.borrow().len(), 0);
	}

	#[test]
	fn element_push_allows_adding_freestanding_element() {
		let msg = ElementPtr::new(None, "message".try_into().unwrap());
		let body = ElementPtr::new(None, "body".try_into().unwrap());
		body.borrow_mut().text("foobar".try_into().unwrap());
		msg.borrow_mut().push(Node::Element(body.clone())).unwrap();
		assert_eq!(msg.borrow().len(), 1);
		assert!(ElementPtr::ptr_eq(&body, &msg.borrow()[0].as_element_ptr().unwrap()));
	}

	#[test]
	fn element_push_allows_adding_child_element_of_unrelated_tree() {
		let msg = ElementPtr::new(None, "message".try_into().unwrap());
		let body = ElementPtr::new(None, "body".try_into().unwrap());
		body.borrow_mut().text("foobar".try_into().unwrap());
		msg.borrow_mut().push(Node::Element(body.clone())).unwrap();
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
		assert_eq!(map_result, Err::<(), _>(MapElementsError::<()>::Structural(InsertError::NodeIsSelf)));
	}

	#[test]
	fn element_map_elements_rejects_insertion_of_parent_at_child() {
		let root = prep_message();
		let act_on = root.borrow()[1].as_element_ptr().unwrap();
		act_on.borrow_mut().tag(None, "dummy".try_into().unwrap(), None);
		assert_eq!(root.borrow().len(), 2);
		assert_eq!(act_on.borrow().len(), 1);
		let map_result = act_on.borrow_mut().map_elements::<_, ()>(|_| {
			Ok(Some(root.clone()))
		});
		assert_eq!(map_result, Err::<(), _>(MapElementsError::<()>::Structural(InsertError::LoopDetected)));
		assert_eq!(root.borrow().len(), 2);
		assert_eq!(act_on.borrow().len(), 1);
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
		assert!(el_ptr.borrow_mut().push(Node::Element(child_ptr)).is_ok());
	}

	#[test]
	fn element_text() {
		let el = ElementPtr::new(None, "message".try_into().unwrap());
		let body_ptr = el.borrow_mut().tag(None, "body".try_into().unwrap(), None);
		let mut body = body_ptr.borrow_mut();
		body.text("Hello World!".try_into().unwrap());
		assert_eq!(body.children.len(), 1);
		assert_eq!(body.children.element_view().len(), 0);
		assert_eq!(body.children[0].as_text().unwrap(), "Hello World!");
	}

	#[test]
	fn element_protect_is_recursive() {
		let el = ElementPtr::new(None, "message".try_into().unwrap());
		el.borrow_mut().tag(None, "body".try_into().unwrap(), None);
		assert!(!el.borrow().is_protected());
		assert!(!el.borrow()[0].as_element_ptr().unwrap().borrow().is_protected());
		el.borrow_mut().protect().unwrap();
		assert!(el.borrow().is_protected());
		assert!(el.borrow()[0].as_element_ptr().unwrap().borrow().is_protected());
	}

	#[test]
	fn element_protect_prohibits_push() {
		let el = ElementPtr::new(None, "message".try_into().unwrap());
		el.borrow_mut().tag(None, "body".try_into().unwrap(), None);
		el.borrow_mut().protect().unwrap();
		let new_node = Node::Element(ElementPtr::new(None, "delay".try_into().unwrap()));
		assert_eq!(el.borrow().len(), 1);
		assert_eq!(el.borrow_mut().push(new_node), Err(InsertError::Protected));
		assert_eq!(el.borrow().len(), 1);
	}

	#[test]
	fn element_protect_prohibits_map_elements() {
		let el = ElementPtr::new(None, "message".try_into().unwrap());
		el.borrow_mut().tag(None, "body".try_into().unwrap(), None);
		el.borrow_mut().protect().unwrap();
		assert_eq!(el.borrow_mut().map_elements::<_, ()>(|_| {
			Ok(None)
		}), Err(MapElementsError::Structural(InsertError::Protected)));
		assert_eq!(el.borrow().len(), 1);
	}

	#[test]
	fn element_protect_allows_tag_and_protects_it() {
		let el = ElementPtr::new(None, "message".try_into().unwrap());
		el.borrow_mut().protect().unwrap();
		el.borrow_mut().tag(None, "body".try_into().unwrap(), None);
		assert_eq!(el.borrow().len(), 1);
		assert!(el.borrow()[0].as_element_ptr().unwrap().borrow().is_protected());
	}

	#[test]
	fn element_protect_allows_text() {
		let el = ElementPtr::new(None, "message".try_into().unwrap());
		el.borrow_mut().protect().unwrap();
		el.borrow_mut().text("foobar2342".try_into().unwrap());
		assert_eq!(el.borrow().len(), 1);
	}

	#[test]
	fn element_unprotected_elements_can_be_inserted_into_separate_trees() {
		let p1 = ElementPtr::new(None, "message".try_into().unwrap());
		let p2 = ElementPtr::new(None, "message".try_into().unwrap());
		let sig = ElementPtr::new(None, "signature".try_into().unwrap());

		p1.borrow_mut().push(Node::Element(sig.clone())).unwrap();
		p2.borrow_mut().push(Node::Element(sig.clone())).unwrap();

		assert_eq!(p1.borrow().len(), 1);
		assert_eq!(p2.borrow().len(), 1);
		assert!(ElementPtr::ptr_eq(
			&p1.borrow()[0].as_element_ptr().unwrap(),
			&p2.borrow()[0].as_element_ptr().unwrap(),
		));
		assert!(ElementPtr::ptr_eq(
			&p1.borrow()[0].as_element_ptr().unwrap(),
			&sig,
		));
	}

	#[test]
	fn element_protected_elements_can_be_inserted_into_separate_trees() {
		let p1 = ElementPtr::new(None, "message".try_into().unwrap());
		let p2 = ElementPtr::new(None, "message".try_into().unwrap());
		let sig = ElementPtr::new(None, "signature".try_into().unwrap());
		sig.borrow_mut().protect().unwrap();

		p1.borrow_mut().push(Node::Element(sig.clone())).unwrap();
		p2.borrow_mut().push(Node::Element(sig.clone())).unwrap();

		assert_eq!(p1.borrow().len(), 1);
		assert_eq!(p2.borrow().len(), 1);
		assert!(ElementPtr::ptr_eq(
			&p1.borrow()[0].as_element_ptr().unwrap(),
			&p2.borrow()[0].as_element_ptr().unwrap(),
		));
		assert!(ElementPtr::ptr_eq(
			&p1.borrow()[0].as_element_ptr().unwrap(),
			&sig,
		));
	}
}
