use mlua::prelude::*;

mod tree;
mod path;
mod stanza;
mod validation;
mod lua;
mod xmpp;

#[mlua::lua_module]
fn util_stanza(lua: &Lua) -> LuaResult<LuaTable> {
	let exports = lua.create_table()?;

	exports.set("stanza", lua.create_function(lua::stanza_new)?)?;
	exports.set("message", lua.create_function(lua::stanza_message)?)?;
	exports.set("iq", lua.create_function(lua::stanza_iq)?)?;
	exports.set("presence", lua.create_function(lua::stanza_presence)?)?;
	exports.set("reply", lua.create_function(lua::stanza_reply)?)?;
	exports.set("error_reply", lua.create_function(lua::stanza_error_reply)?)?;
	exports.set("is_stanza", lua.create_function(lua::stanza_test)?)?;

	Ok(exports)
}
