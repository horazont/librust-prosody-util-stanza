use mlua::prelude::*;

mod stanza;
mod validation;
mod attr;

use crate::stanza::*;
use crate::attr::*;

use std::collections::HashMap;

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

#[derive(Clone)]
struct ElementViewHandle(StanzaPath);

impl ElementViewHandle {
	pub fn wrap(st: StanzaPath) -> ElementViewHandle {
		ElementViewHandle(st)
	}

	pub fn get_index(&self, i: usize) -> Option<usize> {
		let el = self.0.deref_as_element()?;
		el.children.element_view().get_index(i)
	}

	pub fn len(&self) -> Option<usize> {
		let el = self.0.deref_as_element()?;
		Some(el.children.element_view().len())
	}
}

impl LuaUserData for StanzaPath {
	fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
		methods.add_meta_method(LuaMetaMethod::Index, |lua, this, index: IntOrStringArg| -> LuaResult<LuaValue> {
			match this.deref_as_element() {
				None => Err(LuaError::RuntimeError("attempt to index text node".to_string())),
				Some(el) => match index {
					IntOrStringArg::IntArg(i) => {
						if i < 1 {
							return Ok(LuaValue::Nil);
						}
						let real_index = i as usize - 1;
						match el.children.get(real_index) {
							Some(Node::Text(s)) => s.clone().to_lua(lua),
							Some(Node::Element(_)) => this.down(real_index).to_lua(lua),
							None => Ok(LuaValue::Nil),
						}
					},
					IntOrStringArg::StringArg(s) => match s.as_str() {
						"name" => el.localname.clone().to_lua(lua),
						"attr" => AttributePath::wrap(this.clone()).to_lua(lua),
						"tags" => ElementViewHandle::wrap(this.clone()).to_lua(lua),
						_ => Ok(LuaValue::Nil),
					}
				}
			}
		});

		methods.add_meta_method(LuaMetaMethod::Len, |_, this, _: ()| -> LuaResult<usize> {
			match this.deref_as_element() {
				None => Err(LuaError::RuntimeError("attempt to get length of non-element".to_string())),
				Some(el) => Ok(el.children.len()),
			}
		});

		methods.add_method_mut("tag", |_, this, (name, attr): (LuaValue, Option<LuaTable>)| -> LuaResult<StanzaPath> {
			let attr = match attr {
				Some(tbl) => lua_table_to_attr(tbl)?,
				None => HashMap::new(),
			};
			let name = convert_element_name_from_lua(name)?;
			match this.tag(name, attr) {
				Some(path) => Ok(path),
				None => Err(LuaError::RuntimeError("cannot insert element in this place".to_string())),
			}
		});

		methods.add_method_mut("text", |_, this, data: LuaValue| -> LuaResult<StanzaPath> {
			if let LuaValue::Nil = data {
				return Ok(this.clone())
			}
			let data = convert_character_data_from_lua(data)?;
			if data.is_empty() {
				return Ok(this.clone())
			}
			match this.text(data) {
				Some(path) => Ok(path),
				None => Err(LuaError::RuntimeError("cannot insert text in this place".to_string())),
			}
		});

		methods.add_method("up", |_, this, _: ()| -> LuaResult<StanzaPath> {
			match this.up() {
				Some(path) => Ok(path),
				None => Ok(this.clone()),
			}
		});

		methods.add_method_mut("text_tag", |_, this, (name, data): (LuaValue, LuaValue)| -> LuaResult<StanzaPath> {
			let name = convert_element_name_from_lua(name)?;
			let data = convert_character_data_from_lua(data)?;
			match &mut this.tag(name, HashMap::new()) {
				// neither text() nor up() can fail here, we just inserted an
				// element.
				Some(p) => Ok(p.text(data).unwrap().up().unwrap()),
				None => Err(LuaError::RuntimeError("cannot insert element in this place".to_string())),
			}
		});

		methods.add_method("debug", |_, this, _: ()| -> LuaResult<String> {
			Ok(format!("{:?}", this))
		});
	}
}

impl LuaUserData for AttributePath {
	fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
		methods.add_meta_method(LuaMetaMethod::Index, |lua, this, index: String| -> LuaResult<LuaValue> {
			match this.get(index) {
				Some(s) => s.to_lua(lua),
				None => Ok(LuaValue::Nil),
			}
		});

		methods.add_meta_method_mut(LuaMetaMethod::NewIndex, |_, this, (attr, value): (LuaValue, LuaValue)| -> LuaResult<LuaValue> {
			let attr = convert_attribute_name_from_lua(attr)?;
			let value = convert_character_data_from_lua(value)?;
			this.set(attr, value);
			Ok(LuaValue::Nil)
		});
	}
}

impl LuaUserData for ElementViewHandle {
	fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
		methods.add_meta_method(LuaMetaMethod::Index, |lua, this, index: usize| -> LuaResult<LuaValue> {
			if index < 1 {
				return Ok(LuaValue::Nil)
			}
			match this.get_index(index - 1) {
				Some(i) => this.0.down(i).to_lua(lua),
				None => Ok(LuaValue::Nil),
			}
		});

		methods.add_meta_method(LuaMetaMethod::Len, |_, this, _: ()| -> LuaResult<usize> {
			match this.len() {
				Some(i) => Ok(i),
				None => Err(LuaError::RuntimeError(format!("tags refereneced on non-element stanza node"))),
			}
		});
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

fn stanza_new<'l>(lua: &'l Lua, (name, attr): (LuaValue, Option<LuaTable>)) -> LuaResult<LuaValue<'l>> {
	let name = convert_element_name_from_lua(name)?;
	let attr = match attr {
		Some(tbl) => lua_table_to_attr(tbl)?,
		None => HashMap::new(),
	};
	StanzaPath::wrap(Element::new_with_attr(
		name,
		attr,
	)).to_lua(lua)
}

fn stanza_iq<'l>(_: &'l Lua, attr: Option<LuaTable>) -> LuaResult<StanzaPath> {
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

	Ok(StanzaPath::wrap(Element::new_with_attr(
		"iq".to_string(), attr,
	)))
}

fn stanza_message<'l>(_: &'l Lua, (attr, body): (Option<LuaTable>, Option<String>)) -> LuaResult<StanzaPath> {
	let attr = match attr {
		Some(tbl) => lua_table_to_attr(tbl)?,
		None => HashMap::new(),
	};

	let mut st = StanzaPath::wrap(Element::new_with_attr(
		"message".to_string(), attr,
	));
	match body {
		Some(s) => Ok(st.tag("body".to_string(), HashMap::new()).unwrap().text(s).unwrap().reset()),
		None => Ok(st),
	}
}

fn stanza_presence<'l>(_: &'l Lua, (attr, body): (Option<LuaTable>, Option<String>)) -> LuaResult<StanzaPath> {
	let attr = match attr {
		Some(tbl) => lua_table_to_attr(tbl)?,
		None => HashMap::new(),
	};

	Ok(StanzaPath::wrap(Element::new_with_attr(
		"presence".to_string(), attr,
	)))
}

fn checked_stanza<'l>(lua: &'l Lua, v: LuaValue) -> LuaResult<StanzaPath> {
	// TODO: we could use arg to require a StanzaPath as argument right away, but to retain compatibility with the existing test suite, we want to control the error message
	match StanzaPath::from_lua(v, lua) {
		Ok(st) => Ok(st),
		Err(e) => return Err(LuaError::RuntimeError(format!("expected stanza: {}", e))),
	}
}

fn make_reply(st: StanzaPath) -> LuaResult<StanzaPath> {
	let el = match st.deref_as_element() {
		None => return Err(LuaError::RuntimeError("not an element".to_string())),
		Some(el) => el,
	};
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

	Ok(StanzaPath::wrap(Element::new_with_attr(
		el.localname.clone(),
		attr,
	)))
}

fn stanza_reply<'l>(lua: &'l Lua, arg: LuaValue) -> LuaResult<StanzaPath> {
	let st = checked_stanza(lua, arg)?;
	Ok(st)
}

fn stanza_error_reply<'l>(lua: &'l Lua, arg: LuaValue) -> LuaResult<StanzaPath> {
	let st = checked_stanza(lua, arg)?;
	{
		let el = match st.deref_as_element() {
			None => return Err(LuaError::RuntimeError("not an element".to_string())),
			Some(el) => el,
		};
		match el.attr.get("type") {
			Some(s) => match s.as_str() {
				"error" => return Err(LuaError::RuntimeError("bad argument to error_reply: got stanza of type error which must not be replied to".to_string())),
				_ => (),
			},
			None => (),
		}
	}
	let mut result = make_reply(st)?;
	{
		let mut el = result.deref_as_element_mut().unwrap();
		el.attr.insert("type".to_string(), "error".to_string());
	}
	Ok(result)
}

fn stanza_test<'l>(lua: &'l Lua, arg: LuaValue) -> LuaResult<bool> {
	Ok(match StanzaPath::from_lua(arg, lua) {
		Ok(_) => true,
		Err(_) => false,
	})
}

#[mlua::lua_module]
fn util_stanza(lua: &Lua) -> LuaResult<LuaTable> {
	let exports = lua.create_table()?;

	exports.set("stanza", lua.create_function(stanza_new)?)?;
	exports.set("iq", lua.create_function(stanza_iq)?)?;
	exports.set("reply", lua.create_function(stanza_reply)?)?;
	exports.set("error_reply", lua.create_function(stanza_error_reply)?)?;
	exports.set("message", lua.create_function(stanza_message)?)?;
	exports.set("presence", lua.create_function(stanza_presence)?)?;
	exports.set("is_stanza", lua.create_function(stanza_test)?)?;

	Ok(exports)
}
