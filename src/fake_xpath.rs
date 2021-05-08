use std::cell::Ref;
use crate::tree;
use crate::xmpp;

pub fn find<'a>(root: Ref<'a, tree::Element>, path: &str) -> Option<tree::Node> {
	let mut xmlns: Option<String> = None;
	let path = match path.get(0usize..1usize).unwrap_or("") {
		"{" => {
			if let Some(end_pos) = path.find("}") {
				xmlns = Some(path[1..end_pos].to_string());
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
	use crate::stanza;

	const TEST_XMLNS: &str = "urn:uuid:81b8253b-ba8c-4c91-8d3a-f2c6c10c7bfe";

	fn build_tree() -> tree::ElementPtr {
		let mut st = stanza::Stanza::wrap(tree::ElementPtr::new(
			"root".to_string(),
		));
		st.try_deref().unwrap().borrow_mut().attr.insert("foo".to_string(), "bar".to_string());
		st.tag("child1".to_string(), None);
		st.tag("nested1".to_string(), None);
		st.text("Hello World from nested1!".to_string());
		st.up();
		st.tag("nested2".to_string(), None);
		st.text("Hello World from nested2!".to_string());
		st.reset();

		{
			let child2_ptr = st.tag("child2".to_string(), None).unwrap();
			let mut child2 = child2_ptr.borrow_mut();
			child2.attr.insert("attr2".to_string(), "value2".to_string());
			child2.attr.insert("xmlns".to_string(), TEST_XMLNS.to_string());
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
