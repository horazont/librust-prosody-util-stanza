use mlua::prelude::*;
use std::rc::Rc;
use std::cell::{RefCell, Ref, RefMut};
use std::collections::HashMap;
use crate::stanza;
use crate::tree;
use crate::xmpp;
use crate::fake_xpath;
use crate::xml;
use crate::lua_convert::*;
use crate::lua_serialize_compat::{Preserialize, Deserialize};

enum IntOrStringArg {
	IntArg(i64),
	StringArg(String),
}

impl FromLua<'_> for IntOrStringArg {
	fn from_lua<'l>(value: LuaValue, lua: &'l Lua) -> LuaResult<IntOrStringArg> {
		match lua.coerce_integer(value.clone()) {
			Ok(Some(i)) => Ok(IntOrStringArg::IntArg(i)),
			Ok(None) | Err(_) => match lua.coerce_string(value) {
				Ok(Some(luastr)) => Ok(IntOrStringArg::StringArg(luastr.to_str()?.to_string())),
				Ok(None) => Err(LuaError::RuntimeError("expected number or string".to_string())),
				Err(v) => Err(v),
			}
		}
	}
}

impl FromLua<'_> for tree::Node {
	fn from_lua<'l>(value: LuaValue, lua: &'l Lua) -> LuaResult<tree::Node> {
		match LuaStanza::from_lua(value.clone(), lua) {
			Ok(st) => Ok(tree::Node::Element(st.0.borrow().root_ptr().clone())),
			Err(_) => {
				match lua.coerce_string(value.clone()) {
					Ok(Some(s)) => Ok(tree::Node::Text(s.to_str()?.to_string())),
					Ok(None) => Err(LuaError::RuntimeError(format!("expected string or stanza, got {}", value.type_name()))),
					Err(err) => Err(LuaError::RuntimeError(format!("expected string or stanza, got {} ({})", value.type_name(), err))),
				}
			}
		}
	}
}

#[derive(Clone)]
pub struct LuaAttrHandle(tree::ElementPtr);

impl LuaAttrHandle {
	pub fn wrap(el: tree::ElementPtr) -> LuaAttrHandle {
		LuaAttrHandle(el)
	}
}

impl LuaUserData for LuaAttrHandle {
	fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
		methods.add_meta_method(LuaMetaMethod::Index, |lua, this, index: String| -> LuaResult<LuaValue> {
			match this.0.borrow().attr.get(&index) {
				Some(v) => v.clone().to_lua(lua),
				None => Ok(LuaValue::Nil),
			}
		});

		methods.add_meta_method_mut(LuaMetaMethod::NewIndex, |_, this, (key, value): (LuaValue, LuaValue)| -> LuaResult<LuaValue> {
			let key = convert_attribute_name_from_lua(key)?;
			match value {
				LuaValue::Nil => {
					this.0.borrow_mut().attr.remove(&key);
					Ok(LuaValue::Nil)
				},
				_ => {
					let value = convert_character_data_from_lua(value)?;
					this.0.borrow_mut().attr.insert(key, value);
					Ok(LuaValue::Nil)
				}
			}
		});

		methods.add_meta_method(LuaMetaMethod::Eq, |_, this, other: LuaAttrHandle| -> LuaResult<bool> {
			let this_attr = &this.0.borrow().attr;
			let other_attr = &other.0.borrow().attr;
			Ok(*this_attr == *other_attr)
		});
	}
}

#[derive(Clone)]
pub struct LuaNamespacesHandle(tree::ElementPtr);

impl LuaNamespacesHandle {
	pub fn wrap(el: tree::ElementPtr) -> LuaNamespacesHandle {
		LuaNamespacesHandle(el)
	}
}

impl LuaUserData for LuaNamespacesHandle {
	fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
		methods.add_meta_method(LuaMetaMethod::Index, |lua, this, index: String| -> LuaResult<LuaValue> {
			match this.0.borrow().namespaces.get(&index) {
				Some(v) => v.clone().to_lua(lua),
				None => Ok(LuaValue::Nil),
			}
		});

		methods.add_meta_method(LuaMetaMethod::Eq, |_, this, other: LuaAttrHandle| -> LuaResult<bool> {
			let this_namespaces = &this.0.borrow().namespaces;
			let other_namespaces = &other.0.borrow().namespaces;
			Ok(*this_namespaces == *other_namespaces)
		});
	}
}

#[derive(Clone)]
pub struct LuaChildElementViewHandle(tree::ElementPtr);

impl LuaChildElementViewHandle {
	pub fn wrap(el: tree::ElementPtr) -> LuaChildElementViewHandle {
		LuaChildElementViewHandle(el)
	}
}

impl LuaUserData for LuaChildElementViewHandle {
	fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
		methods.add_meta_method(LuaMetaMethod::Len, |_, this, _: ()| -> LuaResult<usize> {
			Ok(this.0.borrow().element_view().len())
		});

		methods.add_meta_method(LuaMetaMethod::Index, |lua, this, index: i64| -> LuaResult<LuaValue> {
			if index < 1 {
				return Ok(LuaValue::Nil);
			}

			let el = this.0.borrow();
			let rust_index = index as usize - 1;
			let el_index = match el.element_view().get_index(rust_index) {
				Some(i) => i,
				None => return Ok(LuaValue::Nil),
			};
			let node = el.get(el_index);
			match node {
				Some(tree::Node::Element(el)) => LuaStanza::wrap(stanza::Stanza::wrap(el.clone())).to_lua(lua),
				_ => Err(LuaError::RuntimeError("internal stanza state corruption: index does not refer to element".to_string()))
			}
		});

		methods.add_meta_method(LuaMetaMethod::Custom("__ipairs".to_string()), |lua, this, _: ()| -> LuaResult<(LuaValue, LuaChildTagsIteratorState)> {
			let root_ptr = &this.0;
			let iterator = LuaChildTagsIteratorState::wrap(root_ptr.clone());
			Ok((lua.create_function(|_, state: LuaChildTagsIteratorState| -> LuaResult<(Option<usize>, Option<LuaStanza>)> {
				let mut state = state.borrow_mut();
				let child_opt = {
					let parent = state.el.borrow();
					let el_view = parent.element_view();
					el_view.get(state.next_index)
				};
				match child_opt {
					Some(child_ptr) => {
						state.next_index += 1;
						// returning index + 1 in accordance with how lua
						// behaves
						Ok((Some(state.next_index), Some(LuaStanza::wrap(stanza::Stanza::wrap(child_ptr.clone())))))
					},
					None => Ok((None, None)),
				}
			})?.to_lua(lua)?, iterator))
		});

		methods.add_meta_method_mut(LuaMetaMethod::NewIndex, |_, this, (index, st): (i64, LuaStanza)| -> LuaResult<LuaValue> {
			if index < 1 {
				return Ok(LuaValue::Nil);
			}

			let mut el = this.0.borrow_mut();
			let rust_index = index as usize - 1;
			let el_view = el.element_view();
			if el_view.len() == rust_index {
				// quirky append, TODO
				return Err(LuaError::RuntimeError("append not implemented yet".to_string()));
			} else {
				// TODO: validate that this is cycle-free
				let el_index = match el_view.get_index(rust_index) {
					None => return Ok(LuaValue::Nil),
					Some(i) => i,
				};
				el[el_index] = tree::Node::Element(st.0.borrow().root_ptr().clone());
			}
			Ok(LuaValue::Nil)
		});
	}
}

struct ChildTagsIteratorState {
	el: tree::ElementPtr,
	next_index: usize,
	selector: Option<xmpp::ElementSelector>,
}

#[derive(Clone)]
struct LuaChildTagsIteratorState(Rc<RefCell<ChildTagsIteratorState>>);

impl LuaChildTagsIteratorState {
	fn wrap(el: tree::ElementPtr) -> LuaChildTagsIteratorState {
		LuaChildTagsIteratorState(Rc::new(RefCell::new(ChildTagsIteratorState{
			el: el,
			next_index: 0,
			selector: None,
		})))
	}

	fn wrap_with_selector(el: tree::ElementPtr, name: Option<String>, xmlns: Option<String>) -> LuaChildTagsIteratorState {
		let selector = xmpp::ElementSelector::select_inside_parent(el.borrow(), name, xmlns);
		LuaChildTagsIteratorState(Rc::new(RefCell::new(ChildTagsIteratorState{
			el: el,
			next_index: 0,
			selector: Some(selector),
		})))
	}

	fn borrow_mut<'a>(&'a self) -> RefMut<'a, ChildTagsIteratorState> {
		self.0.borrow_mut()
	}
}

impl LuaUserData for LuaChildTagsIteratorState {
	fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(_methods: &mut M) {
	}
}

struct ChildNodeIteratorState {
	el: tree::ElementPtr,
	next_index: usize,
}

#[derive(Clone)]
struct LuaChildNodeIteratorState(Rc<RefCell<ChildNodeIteratorState>>);

impl LuaChildNodeIteratorState {
	fn wrap(el: tree::ElementPtr) -> LuaChildNodeIteratorState {
		LuaChildNodeIteratorState(Rc::new(RefCell::new(ChildNodeIteratorState{
			el: el,
			next_index: 0,
		})))
	}

	fn borrow_mut<'a>(&'a self) -> RefMut<'a, ChildNodeIteratorState> {
		self.0.borrow_mut()
	}
}

impl LuaUserData for LuaChildNodeIteratorState {
	fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(_methods: &mut M) {
	}
}

#[derive(Clone)]
pub struct LuaStanza(Rc<RefCell<stanza::Stanza>>);

impl LuaStanza {
	pub fn wrap(st: stanza::Stanza) -> LuaStanza {
		LuaStanza(Rc::new(RefCell::new(st)))
	}

	pub fn wrap_el(el: tree::ElementPtr) -> LuaStanza {
		LuaStanza::wrap(stanza::Stanza::wrap(el))
	}
}

impl<'a> ToLua<'a> for tree::Node {
	fn to_lua(self, lua: &'a Lua) -> LuaResult<LuaValue> {
		match self {
			tree::Node::Text(s) => s.to_lua(lua),
			tree::Node::Element(el) => LuaStanza::wrap(stanza::Stanza::wrap(el)).to_lua(lua),
		}
	}
}

impl LuaUserData for LuaStanza {
	fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
		methods.add_meta_method(LuaMetaMethod::Index, |lua, this, index: IntOrStringArg| -> LuaResult<LuaValue> {
			match index {
				IntOrStringArg::IntArg(i) => {
					if i < 1 {
						return Ok(LuaValue::Nil);
					}
					let real_index = i as usize - 1;
					match this.0.borrow().root().get(real_index) {
						Some(node) => node.clone().to_lua(lua),
						None => Ok(LuaValue::Nil),
					}
				},
				IntOrStringArg::StringArg(s) => match s.as_str() {
					"name" => this.0.borrow().root().localname.clone().to_lua(lua),
					"attr" => LuaAttrHandle::wrap(this.0.borrow().root_ptr()).to_lua(lua),
					"tags" => LuaChildElementViewHandle::wrap(this.0.borrow().root_ptr()).to_lua(lua),
					"namespaces" => LuaNamespacesHandle::wrap(this.0.borrow().root_ptr()).to_lua(lua),
					_ => Ok(LuaValue::Nil),
				}
			}
		});

		methods.add_meta_method(LuaMetaMethod::Len, |_, this, _: ()| -> LuaResult<usize> {
			Ok(this.0.borrow().root().len())
		});

		methods.add_method("text", |_, this, data: LuaValue| -> LuaResult<LuaStanza> {
			let data = convert_optional_character_data_from_lua(data)?;
			match data {
				Some(text) => match this.0.borrow_mut().text(text) {
					true => Ok(this.clone()),
					false => Err(LuaError::RuntimeError("invalid cursor in stanza".to_string())),
				},
				None => Ok(this.clone()),
			}
		});

		methods.add_method("tag", |_, this, (name, attr, namespaces): (LuaValue, Option<LuaTable>, Option<LuaTable>)| -> LuaResult<LuaStanza> {
			let name = convert_element_name_from_lua(name)?;
			let attr = match attr {
				Some(tbl) => Some(lua_table_to_attr(tbl)?),
				None => None,
			};
			let namespaces = match namespaces {
				Some(tbl) => Some(lua_table_to_attr(tbl)?),
				None => None,
			};
			match this.0.borrow_mut().tag(name, attr) {
				Some(el) => {
					if let Some(nss) = namespaces {
						el.borrow_mut().namespaces = nss
					}
					Ok(this.clone())
				},
				None => Err(LuaError::RuntimeError("invalid cursor in stanza".to_string())),
			}
		});

		methods.add_method("up", |_, this, _: ()| -> LuaResult<LuaStanza> {
			this.0.borrow_mut().up();
			Ok(this.clone())
		});

		methods.add_method("text_tag", |_, this, (name, text, attr): (LuaValue, LuaValue, Option<LuaTable>)| -> LuaResult<LuaStanza> {
			let name = convert_element_name_from_lua(name)?;
			let text = convert_character_data_from_lua(text)?;
			let attr = match attr {
				Some(tbl) => Some(lua_table_to_attr(tbl)?),
				None => None,
			};
			let mut this_st = this.0.borrow_mut();
			match this_st.tag(name, attr) {
				Some(new_el) => {
					new_el.borrow_mut().text(text);
					this_st.up();
					Ok(this.clone())
				},
				None => Err(LuaError::RuntimeError("invalid cursor in stanza".to_string())),
			}
		});

		methods.add_method("reset", |_, this, _: ()| -> LuaResult<LuaStanza> {
			this.0.borrow_mut().reset();
			Ok(this.clone())
		});

		methods.add_method("maptags", |_, this, cb: LuaFunction| -> LuaResult<LuaStanza> {
			let st = this.0.borrow();
			let root_ptr = st.root_ptr();
			let mut el = root_ptr.borrow_mut();
			let result = el.map_elements(|el| {
				let st = LuaStanza::wrap(stanza::Stanza::wrap(el));
				match cb.call::<_, Option<LuaStanza>>(st) {
					Err(e) => Err(e),
					Ok(None) => Ok(None),
					Ok(Some(st)) => Ok(Some(st.0.borrow().root_ptr().clone())),
				}
			});
			match result {
				Err(tree::MapElementsError::External(e)) => Err(e),
				Err(tree::MapElementsError::Structural(e)) => Err(LuaError::RuntimeError(format!("structural error during maptags: {}", e))),
				Ok(_) => Ok(this.clone()),
			}
		});

		methods.add_method("remove_children", |_, this, (name, xmlns): (Option<String>, Option<String>)| -> LuaResult<LuaStanza> {
			let st = this.0.borrow();
			let root_ptr = st.root_ptr();
			let selector = xmpp::ElementSelector::select_inside_parent(
				root_ptr.borrow(),
				name,
				xmlns,
			);
			let mut el = root_ptr.borrow_mut();
			let result = el.map_elements::<_, LuaError>(|el| {
				let result = selector.select(Ref::clone(&el.borrow()));
				match result {
					true => Ok(None),
					false => Ok(Some(el)),
				}
			});
			match result {
				Err(tree::MapElementsError::External(e)) => Err(e),
				Err(tree::MapElementsError::Structural(e)) => Err(LuaError::RuntimeError(format!("structural error during maptags: {}", e))),
				Ok(_) => Ok(this.clone()),
			}
		});

		methods.add_method("get_child", |lua, this, (name, xmlns): (Option<String>, Option<String>)| -> LuaResult<LuaValue> {
			let st = this.0.borrow();
			let root_ptr = st.root_ptr();
			match xmpp::find_first_child(root_ptr.borrow(), name, xmlns) {
				Some(child_ptr) => LuaStanza::wrap(stanza::Stanza::wrap(child_ptr)).to_lua(lua),
				None => Ok(LuaValue::Nil)
			}
		});

		methods.add_method("get_child_text", |lua, this, (name, xmlns): (Option<String>, Option<String>)| -> LuaResult<LuaValue> {
			let st = this.0.borrow();
			let root_ptr = st.root_ptr();
			match xmpp::find_first_child(root_ptr.borrow(), name, xmlns) {
				Some(child_ptr) => {
					match child_ptr.borrow().get_text() {
						Some(s) => s.to_lua(lua),
						None => Ok(LuaValue::Nil),
					}
				},
				None => Ok(LuaValue::Nil)
			}
		});

		methods.add_method("get_error", |_, this, _: ()| -> LuaResult<(Option<String>, Option<String>, Option<String>, Option<LuaStanza>)> {
			let st = this.0.borrow();
			let root_ptr = st.root_ptr();
			match xmpp::extract_error(root_ptr.borrow()) {
				Some((type_, condition, text, None)) => Ok((Some(type_), Some(condition), text, None)),
				Some((type_, condition, text, Some(el))) => Ok((Some(type_), Some(condition), text, Some(LuaStanza::wrap(stanza::Stanza::wrap(el))))),
				None => Ok((None, None, None, None)),
			}
		});

		methods.add_method("query", |_, this, xmlns: LuaValue| -> LuaResult<LuaStanza> {
			let xmlns = convert_character_data_from_lua(xmlns)?;
			let mut st = this.0.borrow_mut();
			let mut attr = HashMap::new();
			attr.insert("xmlns".to_string(), xmlns.to_string());
			st.tag("query".to_string(), Some(attr));
			Ok(this.clone())
		});

		methods.add_method("childtags", |lua, this, (name, xmlns): (Option<String>, Option<String>)| -> LuaResult<(LuaValue, LuaChildTagsIteratorState)> {
			let st = this.0.borrow();
			let root_ptr = st.root_ptr();
			let iterator = LuaChildTagsIteratorState::wrap_with_selector(root_ptr, name, xmlns);
			Ok((lua.create_function(|_, state: LuaChildTagsIteratorState| -> LuaResult<Option<LuaStanza>> {
				let mut state = state.borrow_mut();
				loop {
					let child_opt = {
						let parent = state.el.borrow();
						let el_view = parent.element_view();
						el_view.get(state.next_index)
					};
					match child_opt {
						Some(child_ptr) => {
							state.next_index += 1;
							if state.selector.as_ref().unwrap().select(child_ptr.borrow()) {
								return Ok(Some(LuaStanza::wrap(stanza::Stanza::wrap(child_ptr.clone()))))
							}
						},
						None => return Ok(None),
					}
				}
			})?.to_lua(lua)?, iterator))
		});

		methods.add_method("get_text", |_, this, _: ()| -> LuaResult<Option<String>> {
			let st = this.0.borrow();
			let root_ptr = st.root_ptr();
			let root = root_ptr.borrow();
			Ok(root.get_text())
		});

		methods.add_meta_method(LuaMetaMethod::Eq, |_, this, other: LuaStanza| -> LuaResult<bool> {
			let this_st = &this.0;
			let other_st = &other.0;
			Ok(Rc::ptr_eq(this_st, other_st) || *this_st.borrow() == *other_st.borrow())
		});

		methods.add_method("debug", |_, this, _: ()| -> LuaResult<String> {
			Ok(format!("{:#?}", this.0))
		});

		methods.add_method("add_child", |_, this, child: tree::Node| -> LuaResult<LuaStanza> {
			let st = this.0.borrow();
			let target_ptr = match st.try_deref() {
				Some(p) => p,
				None => return Err(LuaError::RuntimeError(format!("stanza position points at invalid element"))),
			};
			target_ptr.borrow_mut().push(child);
			Ok(this.clone())
		});

		methods.add_method("add_direct_child", |_, this, child: tree::Node| -> LuaResult<LuaValue> {
			let st = this.0.borrow();
			let root_ptr = st.root_ptr();
			root_ptr.borrow_mut().push(child);
			Ok(LuaValue::Nil)
		});

		methods.add_method("find", |lua, this, path: String| -> LuaResult<LuaValue> {
			let st = this.0.borrow();
			let root_ptr = st.root_ptr();
			match fake_xpath::find(root_ptr.borrow(), path.as_str()) {
				Some(tree::Node::Text(s)) => s.to_lua(lua),
				Some(tree::Node::Element(el)) => LuaStanza::wrap_el(el).to_lua(lua),
				None => Ok(LuaValue::Nil)
			}
		});

		methods.add_method("top_tag", |_, this, _: ()| -> LuaResult<String> {
			let st = this.0.borrow();
			let root_ptr = st.root_ptr();
			Ok(xml::head_as_str(root_ptr.borrow()).unwrap())
		});

		methods.add_method("at_top", |_, this, _: ()| -> LuaResult<bool> {
			let st = this.0.borrow();
			Ok(st.is_at_top())
		});

		methods.add_method("indent", |_, this, (initial_level, indent): (Option<usize>, Option<String>)| -> LuaResult<String> {
			let indent = indent.unwrap_or("\t".to_string());
			let root_ptr = this.0.borrow().root_ptr();
			let root = root_ptr.borrow();
			let formatter = xml::Formatter{ indent: Some(indent), initial_level: initial_level.unwrap_or(1) };
			match formatter.format(root) {
				Ok(s) => Ok(s),
				Err(e) => Err(LuaError::RuntimeError(format!("failed to indent stanza: {}", e))),
			}
		});

		methods.add_meta_method(LuaMetaMethod::ToString, |_, this, _: ()| -> LuaResult<String> {
			let root_ptr = this.0.borrow().root_ptr();
			let root = root_ptr.borrow();
			let formatter = xml::Formatter{ indent: None, initial_level: 0 };
			match formatter.format(root) {
				Ok(s) => Ok(s),
				Err(e) => Err(LuaError::RuntimeError(format!("failed to format stanza: {}", e))),
			}
		});

		methods.add_meta_method(LuaMetaMethod::Custom("__ipairs".to_string()), |lua, this, _: ()| -> LuaResult<(LuaValue, LuaChildNodeIteratorState)> {
			let root_ptr = this.0.borrow().root_ptr();
			let iterator = LuaChildNodeIteratorState::wrap(root_ptr.clone());
			Ok((lua.create_function(|lua, state: LuaChildNodeIteratorState| -> LuaResult<(Option<usize>, Option<LuaValue>)> {
				let mut state = state.borrow_mut();
				state.next_index += 1;
				let parent = state.el.borrow();
				let child_opt = parent.get(state.next_index);
				if let None = child_opt {
					drop(parent);
					state.next_index -= 1;
					return Ok((None, None));
				}
				let child_node = child_opt.unwrap();
				// returning index + 1 in accordance with how lua
				// behaves
				match child_node {
					tree::Node::Text(s) => Ok((Some(state.next_index), Some(s.clone().to_lua(lua)?))),
					tree::Node::Element(e) => Ok((Some(state.next_index), Some(LuaStanza::wrap_el(e.clone()).to_lua(lua)?))),
				}
			})?.to_lua(lua)?, iterator))
		});
	}
}

pub fn stanza_new<'l>(_: &'l Lua, (name, attr): (LuaValue, Option<LuaTable>)) -> LuaResult<LuaStanza> {
	let name = convert_element_name_from_lua(name)?;
	let attr = match attr {
		Some(tbl) => Some(lua_table_to_attr(tbl)?),
		None => None,
	};
	Ok(LuaStanza::wrap(
		stanza::Stanza::new(name, attr),
	))
}

pub fn stanza_message<'l>(_: &'l Lua, (attr, body): (Option<LuaTable>, LuaValue)) -> LuaResult<LuaStanza> {
	let attr = match attr {
		Some(tbl) => Some(lua_table_to_attr(tbl)?),
		None => None,
	};

	let body = convert_optional_character_data_from_lua(body)?;

	let mut st = stanza::Stanza::new("message".to_string(), attr);
	match body {
		Some(s) => {
			st.tag("body".to_string(), None);
			st.text(s);
			st.up();
		},
		_ => (),
	}
	Ok(LuaStanza::wrap(st))
}

pub fn stanza_iq<'l>(_: &'l Lua, attr: Option<LuaTable>) -> LuaResult<LuaStanza> {
	let attr = match attr {
		Some(tbl) => lua_table_to_attr(tbl)?,
		None => return Err(LuaError::RuntimeError("iq stanzas require id and type attributes".to_string())),
	};

	if !attr.contains_key("id") {
		return Err(LuaError::RuntimeError("iq stanzas require an id attribute".to_string()))
	}

	if !attr.contains_key("type") {
		return Err(LuaError::RuntimeError("iq stanzas require a type attribute".to_string()))
	}

	Ok(LuaStanza::wrap(stanza::Stanza::new(
		"iq".to_string(), Some(attr),
	)))
}

pub fn stanza_presence<'l>(_: &'l Lua, attr: Option<LuaTable>) -> LuaResult<LuaStanza> {
	let attr = match attr {
		Some(tbl) => Some(lua_table_to_attr(tbl)?),
		None => None,
	};

	Ok(LuaStanza::wrap(stanza::Stanza::new(
		"presence".to_string(), attr,
	)))
}

fn checked_stanza<'l>(lua: &'l Lua, v: LuaValue) -> LuaResult<LuaStanza> {
	// TODO: we could use arg to require a StanzaPath as argument right away, but to retain compatibility with the existing test suite, we want to control the error message
	match LuaStanza::from_lua(v, lua) {
		Ok(st) => Ok(st),
		Err(e) => return Err(LuaError::RuntimeError(format!("expected stanza: {}", e))),
	}
}

pub fn stanza_reply<'l>(lua: &'l Lua, st: LuaValue) -> LuaResult<LuaStanza> {
	let st = checked_stanza(lua, st)?;
	let result = xmpp::make_reply(st.0.borrow().root_ptr().borrow());
	Ok(LuaStanza::wrap(stanza::Stanza::wrap(result)))
}

fn process_util_error_extra<'a>(condition: String, mut error: RefMut<'a, tree::Element>, extra: LuaTable) {
	if condition == "gone" {
		// check for uri in table, if it exists, we add it as text to
		// the condition node
		extra.get::<_, LuaValue>("uri").and_then(|v| {
			convert_optional_character_data_from_lua(v)
		}).and_then(|uri| {
			uri.and_then(|uri| {
				error.element_view().get(0).and_then::<(), _>(|el| {
					el.borrow_mut().text(uri);
					None
				})
			});
			Ok(())
		}).ok();
	}
	match extra.get::<_, LuaStanza>("tag") {
		Ok(st) => {
			error.push(tree::Node::Element(st.0.borrow().root_ptr()));
		},
		_ => {
			extra.get::<_, LuaValue>("namespace").and_then(|nsv| {
				convert_character_data_from_lua(nsv)
			}).and_then(|ns| {
				Ok((ns, extra.get::<_, LuaValue>("condition").and_then(|cv| {
					convert_element_name_from_lua(cv)
				})?))
			}).and_then(|(ns, condition)| {
				let el = error.tag(
					condition,
					None,
				);
				el.borrow_mut().attr.insert("xmlns".to_string(), ns);
				Ok(())
			}).ok();
		}
	}
}

fn stanza_error_reply_from_util_error<'l>(_: &'l Lua, (st, error_table): (LuaStanza, LuaTable)) -> LuaResult<LuaStanza> {
	let type_ = convert_character_data_from_lua(error_table.get::<_, LuaValue>("type")?)?;
	let condition = convert_element_name_from_lua(error_table.get::<_, LuaValue>("condition")?)?;
	let text = convert_optional_character_data_from_lua(error_table.get::<_, LuaValue>("text")?)?;
	let by = {
		match error_table.get::<_, Option<LuaTable>>("context")? {
			Some(t) => convert_optional_character_data_from_lua(t.get::<_, LuaValue>("by")?)?,
			None => None,
		}
	};
	let extra = match error_table.get::<_, LuaTable>("extra") {
		// we clone the condition here because we need it later on
		Ok(tbl) => Some((condition.clone(), tbl)),
		Err(_) => None,
	};

	let st_root = st.0.borrow().root_ptr();
	match xmpp::make_error_reply(
		st_root.borrow(),
		type_,
		condition,
		text,
		by,
	) {
		Ok(r) => {
			let mut st = stanza::Stanza::wrap(r);
			st.down(0);
			if let Some((condition, extra_tbl)) = extra {
				process_util_error_extra(condition, st.try_deref().unwrap().borrow_mut(), extra_tbl);
			}
			Ok(LuaStanza::wrap(st))
		},
		Err(s) => Err(LuaError::RuntimeError(s)),
	}
}

pub fn stanza_error_reply<'l>(lua: &'l Lua, (st, type_, condition, text, by): (LuaValue, LuaValue, LuaValue, LuaValue, LuaValue)) -> LuaResult<LuaStanza> {
	let st = checked_stanza(lua, st)?;
	let type_ = match type_ {
		LuaValue::Table(t) => {
			return stanza_error_reply_from_util_error(lua, (st, t));
		}
		other => convert_character_data_from_lua(other)?
	};
	let condition = convert_element_name_from_lua(condition).unwrap_or("undefined-condition".to_string());
	let text = convert_optional_character_data_from_lua(text)?;
	let by = convert_optional_character_data_from_lua(by)?;
	let st_root = st.0.borrow().root_ptr();
	match xmpp::make_error_reply(
		st_root.borrow(),
		type_,
		condition,
		text,
		by,
	) {
		Ok(r) => {
			let mut st = stanza::Stanza::wrap(r);
			st.down(0);
			Ok(LuaStanza::wrap(st))
		},
		Err(s) => Err(LuaError::RuntimeError(s)),
	}
}

pub fn stanza_test<'l>(lua: &'l Lua, st: LuaValue) -> LuaResult<bool> {
	Ok(match LuaStanza::from_lua(st, lua) {
		Ok(_) => true,
		Err(_) => false,
	})
}

pub fn stanza_clone<'l>(lua: &'l Lua, st: LuaValue) -> LuaResult<LuaStanza> {
	Ok(LuaStanza::wrap(checked_stanza(lua, st)?.0.borrow().deep_clone()))
}

pub fn stanza_preserialize<'l>(lua: &'l Lua, st: LuaValue) -> LuaResult<LuaValue<'l>> {
	let st = checked_stanza(lua, st)?;
	let root_ptr = st.0.borrow().root_ptr();
	let root = root_ptr.borrow();
	root.preserialize(lua)
}

pub fn stanza_deserialize<'l>(lua: &'l Lua, t: LuaValue<'l>) -> LuaResult<LuaStanza> {
	Ok(LuaStanza::wrap_el(tree::ElementPtr::deserialize(lua, t)?))
}
