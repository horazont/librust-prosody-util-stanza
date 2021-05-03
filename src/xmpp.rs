use std::cell::Ref;
use std::collections::HashMap;

use crate::tree;

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

	tree::ElementPtr::wrap(tree::Element::new_with_attr(
		el.localname.clone(),
		Some(attr),
	))
}
