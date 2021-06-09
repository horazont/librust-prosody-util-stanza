use super::tree;

#[derive(Clone, PartialEq)]
pub struct ElementPath(Vec<usize>);

impl ElementPath {
	pub fn new() -> ElementPath {
		ElementPath(Vec::new())
	}

	pub fn deref_on<'a>(&self, root: tree::ElementPtr) -> Option<tree::ElementPtr> {
		let mut curr = root;
		for idx in self.0.iter() {
			let idx = *idx;
			curr = curr.clone().borrow().get(idx)?.as_element_ptr()?.clone()
		}
		Some(curr)
	}

	pub fn down(&mut self, index: usize) {
		self.0.push(index);
	}

	pub fn up(&mut self) {
		if self.0.len() == 0 {
			return;
		}
		self.0.pop();
	}

	pub fn reset(&mut self) {
		self.0.clear();
	}

	pub fn depth(&self) -> usize {
		self.0.len()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::convert::TryInto;

	#[test]
	fn element_path_new_derefs_to_root() {
		let p = ElementPath::new();
		let root = tree::ElementPtr::new(None, "root".try_into().unwrap());
		let root_ref = p.deref_on(root.clone());
		assert_eq!(root.borrow().localname, root_ref.unwrap().borrow().localname);
	}

	#[test]
	fn element_path_down_to_ref_to_children() {
		let mut p = ElementPath::new();
		let root = tree::ElementPtr::new(None, "root".try_into().unwrap());
		{
			let mut root_el = root.borrow_mut();
			root_el.tag(None, "body".try_into().unwrap(), None).borrow_mut().text("foobar".try_into().unwrap());
		}

		p.down(0);
		let node_ref = p.deref_on(root.clone());
		assert_eq!("body", node_ref.unwrap().borrow().localname);
	}

	#[test]
	fn element_path_up_to_go_back() {
		let mut p = ElementPath::new();
		let root = tree::ElementPtr::new(None, "root".try_into().unwrap());
		{
			let mut root_el = root.borrow_mut();
			root_el.tag(None, "body".try_into().unwrap(), None).borrow_mut().text("foobar".try_into().unwrap());
		}

		p.down(0);
		p.up();
		let root_ref = p.deref_on(root.clone());
		assert_eq!(root.borrow().localname, root_ref.unwrap().borrow().localname);
	}


	#[test]
	fn element_path_down_to_ref_to_text_is_none() {
		let mut p = ElementPath::new();
		let root = tree::ElementPtr::new(None, "root".try_into().unwrap());
		{
			let mut root_el = root.borrow_mut();
			root_el.tag(None, "body".try_into().unwrap(), None).borrow_mut().text("foobar".try_into().unwrap());
		}

		p.down(0);
		p.down(0);
		assert!(p.deref_on(root.clone()).is_none());
	}
}
