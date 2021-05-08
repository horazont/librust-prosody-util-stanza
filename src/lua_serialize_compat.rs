use mlua::prelude::*;
use std::collections::HashMap;

use crate::tree;
use crate::lua_convert::*;

pub trait Preserialize {
	fn preserialize<'l>(&self, lua: &'l Lua) -> LuaResult<LuaValue<'l>>;
}

pub trait Deserialize {
	fn deserialize<'l>(lua: &'l Lua, value: LuaValue<'l>) -> LuaResult<Self>
		where Self: Sized;
}

impl Preserialize for tree::Node {
	fn preserialize<'l>(&self, lua: &'l Lua) -> LuaResult<LuaValue<'l>> {
		match self {
			tree::Node::Text(ref s) => s.preserialize(lua),
			tree::Node::Element(ref ptr) => ptr.borrow().preserialize(lua),
		}
	}
}

impl Deserialize for tree::Node {
	fn deserialize<'l>(lua: &'l Lua, value: LuaValue<'l>) -> LuaResult<Self> {
		match value {
			LuaValue::String(s) => Ok(tree::Node::Text(s.to_str()?.to_string())),
			other => Ok(tree::Node::Element(tree::ElementPtr::deserialize(lua, other)?)),
		}
	}
}

impl Preserialize for tree::Element {
	fn preserialize<'l>(&self, lua: &'l Lua) -> LuaResult<LuaValue<'l>> {
		let result = lua.create_table()?;
		result.set("name", self.localname.clone())?;
		result.set("attr", self.attr.preserialize(lua)?)?;
		for (i, child_node) in self.iter().enumerate() {
			let lua_i = i + 1;
			result.set(lua_i, child_node.preserialize(lua)?)?;
		}
		Ok(LuaValue::Table(result))
	}
}

impl Deserialize for tree::ElementPtr {
	fn deserialize<'l>(lua: &'l Lua, value: LuaValue<'l>) -> LuaResult<Self> {
		let value = LuaTable::from_lua(value, lua)?;
		let name = convert_element_name_from_lua(value.get::<_, LuaValue>("name")?)?;
		let attr = lua_table_to_attr(value.get::<_, LuaTable>("attr")?)?;
		let el = tree::ElementPtr::new_with_attr(
			name,
			Some(attr),
		);
		let mut numindex = 1usize;
		{
			let mut el_mut = el.borrow_mut();
			loop {
				let val = value.get::<_, LuaValue>(numindex)?;
				if let LuaValue::Nil = val {
					break;
				}
				el_mut.push(tree::Node::deserialize(lua, val)?);
				numindex += 1;
			}
		}
		Ok(el)
	}
}

impl Preserialize for HashMap<String, String> {
	fn preserialize<'l>(&self, lua: &'l Lua) -> LuaResult<LuaValue<'l>> {
		self.clone().to_lua(lua)
	}
}

impl Preserialize for String {
	fn preserialize<'l>(&self, lua: &'l Lua) -> LuaResult<LuaValue<'l>> {
		self.clone().to_lua(lua)
	}
}
