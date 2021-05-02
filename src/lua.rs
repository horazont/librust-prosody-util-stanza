use mlua::prelude::*;
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use crate::stanza;
use crate::validation;
use crate::tree;

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
			tree::Node::Element(_) => Ok(LuaValue::Nil),
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
					match this.0.borrow().root().children.get(real_index) {
						Some(node) => node.clone().to_lua(lua),
						None => Ok(LuaValue::Nil),
					}
				},
				IntOrStringArg::StringArg(s) => match s.as_str() {
					"name" => this.0.borrow().root().localname.clone().to_lua(lua),
					_ => Ok(LuaValue::Nil),
				}
			}
		});

		methods.add_meta_method(LuaMetaMethod::Len, |_, this, _: ()| -> LuaResult<usize> {
			Ok(this.0.borrow().root().children.len())
		});

		methods.add_method("text", |_, this, data: LuaValue| -> LuaResult<LuaStanza> {
			let data = convert_character_data_from_lua(data)?;
			match this.0.borrow_mut().text(data) {
				true => Ok(this.clone()),
				false => Err(LuaError::RuntimeError("invalid cursor in stanza".to_string())),
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
