use mlua::prelude::*;
use std::borrow::Cow;
use std::collections::HashMap;

use crate::validation;

pub fn strict_string_from_lua<'a>(v: &'a LuaValue) -> LuaResult<&'a [u8]> {
	match &v {
		LuaValue::String(s) => Ok(s.as_bytes()),
		_ => Err(LuaError::FromLuaConversionError{ from: v.type_name(), to: "String", message: None })
		// _ => Err(LuaError::RuntimeError(format!("invalid type: {}", v.type_name()))),
		// _ =>
	}
}

pub fn convert_element_name_from_lua(v: LuaValue) -> LuaResult<String> {
	let raw = strict_string_from_lua(&v)?;
	match validation::convert_xml_element_name(Cow::from(raw)) {
		Ok(s) => Ok(s),
		Err(e) => return Err(LuaError::RuntimeError(format!("invalid element name: {}", e))),
	}
}

pub fn convert_attribute_name_from_lua(v: LuaValue) -> LuaResult<String> {
	let raw = strict_string_from_lua(&v)?;
	match validation::convert_xml_attribute_name(Cow::from(raw)) {
		Ok(s) => Ok(s),
		Err(e) => return Err(LuaError::RuntimeError(format!("invalid attribute name: {}", e))),
	}
}

pub fn convert_character_data_from_lua(v: LuaValue) -> LuaResult<String> {
	let raw = strict_string_from_lua(&v)?;
	match validation::convert_xml_cdata(Cow::from(raw)) {
		Ok(s) => Ok(s),
		Err(e) => return Err(LuaError::RuntimeError(format!("invalid cdata/text: {}", e))),
	}
}

pub fn convert_optional_character_data_from_lua(v: LuaValue) -> LuaResult<Option<String>> {
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

pub fn lua_table_to_attr(tbl: LuaTable) -> LuaResult<HashMap<String, String>> {
	let mut result = HashMap::new();
	for pair in tbl.pairs::<LuaValue, LuaValue>() {
		let (key, value) = pair?;
		let key = convert_attribute_name_from_lua(key)?;
		let value = convert_character_data_from_lua(value)?;
		result.insert(key, value);
	}
	Ok(result)
}