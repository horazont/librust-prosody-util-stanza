use mlua::prelude::*;

pub mod stanza;
pub mod xmppstream;
mod lua_convert;
mod lua_serialize_compat;
mod validation;

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

#[mlua::lua_module]
fn librprosody(lua: &Lua) -> LuaResult<LuaTable> {
	let exports = lua.create_table()?;

	let stanza = lua.create_table()?;
	stanza.set("stanza", lua.create_function(stanza::lua::stanza_new)?)?;
	stanza.set("message", lua.create_function(stanza::lua::stanza_message)?)?;
	stanza.set("iq", lua.create_function(stanza::lua::stanza_iq)?)?;
	stanza.set("presence", lua.create_function(stanza::lua::stanza_presence)?)?;
	stanza.set("reply", lua.create_function(stanza::lua::stanza_reply)?)?;
	stanza.set("error_reply", lua.create_function(stanza::lua::stanza_error_reply)?)?;
	stanza.set("is_stanza", lua.create_function(stanza::lua::stanza_test)?)?;
	stanza.set("clone", lua.create_function(stanza::lua::stanza_clone)?)?;
	stanza.set("preserialize", lua.create_function(stanza::lua::stanza_preserialize)?)?;
	stanza.set("deserialize", lua.create_function(stanza::lua::stanza_deserialize)?)?;
	exports.set("stanza", stanza)?;

	let xmppstream = lua.create_table()?;
	xmppstream.set("new", lua.create_function(xmppstream::lua::stream_new)?)?;
	xmppstream.set("ns_separator", "\x01")?;
	xmppstream.set("ns_pattern", "^([^\x01]*)\x01?(.*)$")?;
	xmppstream.set("rxml_version", rxml::VERSION)?;
	exports.set("xmppstream", xmppstream)?;

	exports.set("version", VERSION)?;

	Ok(exports)
}
