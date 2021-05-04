use mlua::prelude::*;
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use crate::stanza;
use crate::validation;
use crate::tree;
use crate::xmpp;

enum IntOrStringArg {
	IntArg(i64),
	StringArg(String),
}

impl FromLua<'_> for IntOrStringArg {
	fn from_lua<'l>(value: LuaValue, lua: &'l Lua) -> LuaResult<IntOrStringArg> {
		match lua.coerce_integer(value.clone()) {
			Ok(Some(i)) => Ok(IntOrStringArg::IntArg(i)),
			Ok(None) | Err(_) => match lua.coerce_string(value) {
				Ok(Some(luastr)) => match luastr.to_str() {
					Ok(s) => Ok(IntOrStringArg::StringArg(s.to_string())),
					Err(v) => Err(v),
				}
				Ok(None) => Err(LuaError::RuntimeError("expected number or string".to_string())),
				Err(v) => Err(v),
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

#[derive(Clone)]
pub struct LuaStanza(Rc<RefCell<stanza::Stanza>>);

impl LuaStanza {
	pub fn wrap(st: stanza::Stanza) -> LuaStanza {
		LuaStanza(Rc::new(RefCell::new(st)))
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

		methods.add_method("tag", |_, this, (name, attr): (LuaValue, Option<LuaTable>)| -> LuaResult<LuaStanza> {
			let name = convert_element_name_from_lua(name)?;
			let attr = match attr {
				Some(tbl) => Some(lua_table_to_attr(tbl)?),
				None => None,
			};
			match this.0.borrow_mut().tag(name, attr) {
				Some(_) => Ok(this.clone()),
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
				Some(tree::MapElementsError::External(e)) => Err(e),
				Some(tree::MapElementsError::Structural(e)) => Err(LuaError::RuntimeError(format!("structural error during maptags: {}", e))),
				None => Ok(this.clone()),
			}
		});
	}
}

fn strict_string_from_lua<'a>(v: &'a LuaValue) -> LuaResult<&'a [u8]> {
	match &v {
		LuaValue::String(s) => Ok(s.as_bytes()),
		_ => Err(LuaError::RuntimeError(format!("invalid type: {}", v.type_name()))),
	}
}

fn convert_element_name_from_lua(v: LuaValue) -> LuaResult<String> {
	let raw = strict_string_from_lua(&v)?;
	match validation::convert_xml_element_name(raw.as_ref()) {
		Ok(s) => Ok(s),
		Err(e) => return Err(LuaError::RuntimeError(format!("invalid element name: {}", e))),
	}
}

fn convert_attribute_name_from_lua(v: LuaValue) -> LuaResult<String> {
	let raw = strict_string_from_lua(&v)?;
	match validation::convert_xml_attribute_name(raw.as_ref()) {
		Ok(s) => Ok(s),
		Err(e) => return Err(LuaError::RuntimeError(format!("invalid attribute name: {}", e))),
	}
}

fn convert_character_data_from_lua(v: LuaValue) -> LuaResult<String> {
	let raw = strict_string_from_lua(&v)?;
	match validation::convert_xml_cdata(raw.as_ref()) {
		Ok(s) => Ok(s),
		Err(e) => return Err(LuaError::RuntimeError(format!("invalid cdata/text: {}", e))),
	}
}

fn convert_optional_character_data_from_lua(v: LuaValue) -> LuaResult<Option<String>> {
	match v {
		LuaValue::Nil => Ok(None),
		_ => {
			let data = convert_character_data_from_lua(v)?;
			if data.is_empty() {
				Ok(None)
			} else {
				Ok(Some(data))
			}
		}
	}
}

fn lua_table_to_attr(tbl: LuaTable) -> LuaResult<HashMap<String, String>> {
	let mut result = HashMap::new();
	for pair in tbl.pairs::<LuaValue, LuaValue>() {
		let (key, value) = pair?;
		let key = convert_attribute_name_from_lua(key)?;
		let value = convert_character_data_from_lua(value)?;
		result.insert(key, value);
	}
	Ok(result)
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
	Ok(st)
}

pub fn stanza_error_reply<'l>(lua: &'l Lua, st: LuaValue) -> LuaResult<LuaStanza> {
	let st = checked_stanza(lua, st)?;
	let reply;
	{
		let st_deref = st.0.borrow();
		let el = st_deref.root();
		match el.attr.get("type") {
			Some(s) => match s.as_str() {
				"error" => return Err(LuaError::RuntimeError("bad argument to error_reply: got stanza of type error which must not be replied to".to_string())),
				_ => (),
			},
			None => (),
		};
		reply = xmpp::make_reply(el);
	}
	reply.borrow_mut().attr.insert("type".to_string(), "error".to_string());
	Ok(LuaStanza::wrap(stanza::Stanza::wrap(reply)))
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
