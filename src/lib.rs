use mlua::prelude::*;

mod tree;
mod path;
mod stanza;
mod validation;
mod lua;
mod xmpp;
mod fake_xpath;
mod xml;
mod lua_convert;
mod lua_serialize_compat;

#[mlua::lua_module]
fn librprosody(lua: &Lua) -> LuaResult<LuaTable> {
	let exports = lua.create_table()?;

	let stanza = lua.create_table()?;
	stanza.set("stanza", lua.create_function(lua::stanza_new)?)?;
	stanza.set("message", lua.create_function(lua::stanza_message)?)?;
	stanza.set("iq", lua.create_function(lua::stanza_iq)?)?;
	stanza.set("presence", lua.create_function(lua::stanza_presence)?)?;
	stanza.set("reply", lua.create_function(lua::stanza_reply)?)?;
	stanza.set("error_reply", lua.create_function(lua::stanza_error_reply)?)?;
	stanza.set("is_stanza", lua.create_function(lua::stanza_test)?)?;
	stanza.set("clone", lua.create_function(lua::stanza_clone)?)?;
	stanza.set("preserialize", lua.create_function(lua::stanza_preserialize)?)?;
	stanza.set("deserialize", lua.create_function(lua::stanza_deserialize)?)?;
	exports.set("stanza", stanza);

	Ok(exports)
}
