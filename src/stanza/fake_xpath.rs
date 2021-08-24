use std::cell::Ref;
use std::rc::Rc;
use rxml::CData;
use super::tree;
use super::xmpp;

pub fn find<'a>(root: Ref<'a, tree::Element>, path: &str) -> Option<tree::Node> {
	let mut xmlns: Option<Rc<CData>> = None;
	let path = match path.get(0usize..1usize).unwrap_or("") {
		"{" => {
			if let Some(end_pos) = path.find("}") {
				// TODO: find a way to avoid the cdata assertion here
				xmlns = Some(Rc::new(CData::from_string(path[1..end_pos].to_string()).ok()?));
				&path[end_pos+1..]
			} else {
				path
			}
		},
		"@" => {
			// oh, looking for an attribute; let’s look that up and return it
			// then.
			return Some(tree::Node::Text(root.attr.get(&path[1..])?.to_string()))
		},
		_ => path,
	};
	let (name, path) = match path.find(&['#', '@', '/'][..]) {
		Some(end_pos) => match &path[end_pos..end_pos+1] {
			"/" => (&path[..end_pos], &path[end_pos+1..]),
			// if not a slash, keep the remainder intact for the next
			// iteration
			"#" => (&path[..end_pos], &path[end_pos..]),
			_ => (&path[..end_pos], &path[end_pos..]),
		},
		// no further delimiter -> the remainder of the path is the name
		None => (path, ""),
	};

	let child = xmpp::find_first_child(
		root,
		Some(name.to_string()),
		xmlns,
	)?;

	if path == "#" {
		Some(tree::Node::Text(child.borrow().get_text()?))
	} else if path.len() == 0 {
		Some(tree::Node::Element(child))
	} else {
		find(child.borrow(), path)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use super::super::stanza;
	use std::rc::Rc;
	use std::convert::TryInto;
	use rxml::CData;

	const TEST_XMLNS: &str = "urn:uuid:81b8253b-ba8c-4c91-8d3a-f2c6c10c7bfe";

	fn build_tree() -> tree::ElementPtr {
		let mut st = stanza::Stanza::wrap(tree::ElementPtr::new(
			None,
			"root".try_into().unwrap(),
		));
		st.try_deref().unwrap().borrow_mut().attr.insert("foo".try_into().unwrap(), "bar".try_into().unwrap());
		st.tag(None, "child1".try_into().unwrap(), None);
		st.tag(None, "nested1".try_into().unwrap(), None);
		st.text("Hello World from nested1!".try_into().unwrap());
		st.up();
		st.tag(None, "nested2".try_into().unwrap(), None);
		st.text("Hello World from nested2!".try_into().unwrap());
		st.reset();

		{
			let child2_ptr = st.tag(Some(Rc::new(TEST_XMLNS.try_into().unwrap())), "child2".try_into().unwrap(), None).unwrap();
			let mut child2 = child2_ptr.borrow_mut();
			child2.attr.insert("attr2".try_into().unwrap(), "value2".try_into().unwrap());
		}

		st.root_ptr()
	}

	#[test]
	fn find_direct_child() {
		let root = build_tree();
		let result = find(root.borrow(), "child1");
		assert!(result.is_some());
		let result = result.unwrap().as_element_ptr().unwrap();
		assert!(tree::ElementPtr::ptr_eq(&result, &root.borrow()[0].as_element_ptr().unwrap()));
	}

	#[test]
	fn find_direct_child_with_ns() {
		let root = build_tree();
		let result = find(root.borrow(), format!("{{{}}}child2", TEST_XMLNS).as_str());
		assert!(result.is_some());
		let result = result.unwrap().as_element_ptr().unwrap();
		assert!(tree::ElementPtr::ptr_eq(&result, &root.borrow()[1].as_element_ptr().unwrap()));
	}

	#[test]
	fn find_nested_child() {
		let root = build_tree();
		let result = find(root.borrow(), "child1/nested2");
		assert!(result.is_some());
		let result = result.unwrap().as_element_ptr().unwrap();
		assert!(tree::ElementPtr::ptr_eq(&result, &root.borrow()[0].as_element_ptr().unwrap().borrow()[1].as_element_ptr().unwrap()));
	}

	#[test]
	fn find_attribute() {
		let root = build_tree();
		let result = find(root.borrow(), format!("{{{}}}child2@attr2", TEST_XMLNS).as_str());
		assert!(result.is_some());
		let result = result.unwrap().as_text().unwrap().clone();
		assert_eq!(result, "value2");
	}

	#[test]
	fn find_text() {
		let root = build_tree();
		let result = find(root.borrow(), "child1/nested2#");
		assert!(result.is_some());
		let result = result.unwrap().as_text().unwrap().clone();
		assert_eq!(result, "Hello World from nested2!");
	}

}
