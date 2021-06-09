use mlua::prelude::*;
use std::rc::Rc;
use super::stream;
use std::io;
use bstr::BString;
use crate::stanza;

const IDX_SESSION: u32 = 1u32;
const IDX_STREAMOPENED: u32 = 2u32;
const IDX_STREAMCLOSED: u32 = 3u32;
const IDX_STANZA: u32 = 4u32;
const IDX_ERROR: u32 = 5u32;

fn capture_callbacks<'l>(l: &'l Lua, tbl: &LuaTable<'l>) -> LuaResult<LuaTable<'l>> {
	let cbtbl = l.create_table()?;
	cbtbl.raw_set(IDX_STREAMOPENED, tbl.get::<_, LuaValue>("streamopened")?)?;
	cbtbl.raw_set(IDX_STREAMCLOSED, tbl.get::<_, LuaValue>("streamclosed")?)?;
	cbtbl.raw_set(IDX_STANZA, tbl.get::<_, LuaValue>("handlestanza")?)?;
	cbtbl.raw_set(IDX_ERROR, tbl.get::<_, LuaValue>("error")?)?;
	Ok(cbtbl)
}

fn capture_stream_config<'l>(tbl: &LuaTable<'l>) -> LuaResult<stream::StreamConfig> {
	let ctx = tbl.get::<_, Option<ProsodyXmlContext>>("ctx")?.and_then(|v| { Some(v.0) }).unwrap_or_else(|| { Rc::new(rxml::Context::new()) });
	let stream_ns = rxml::CData::from_string(tbl.get::<_, Option<String>>("stream_ns")?.unwrap_or_else(|| { stream::XMLNS_STREAMS.to_string() })).unwrap();
	let stream_localname = tbl.get::<_, Option<String>>("stream_tag")?.unwrap_or_else(|| { "stream".to_string() });
	let error_localname = tbl.get::<_, Option<String>>("error_tag")?.unwrap_or_else(|| { "error".to_string() });
	let default_ns = rxml::CData::from_string(tbl.get::<_, String>("default_ns")?).unwrap();

	Ok(stream::StreamConfig{
		stream_localname: stream_localname,
		error_localname: error_localname,
		default_namespace: ctx.intern_cdata(default_ns),
		stream_namespace: ctx.intern_cdata(stream_ns),
		init_ctx: Some(ctx),
	})
}

impl<'l> ToLua<'l> for stream::Error {
	fn to_lua(self, l: &'l Lua) -> LuaResult<LuaValue<'l>> {
		format!("{}", self).to_lua(l)
	}
}

fn maybe_set<'l, K: ToLua<'l>, T: Into<String>>(tbl: &'l LuaTable, k: K, v: Option<T>) -> LuaResult<()> {
	if let Some(v) = v {
		tbl.raw_set(k, v.into())?;
	}
	Ok(())
}

fn cb_error_or_ret<'l>(cbtbl: &LuaTable<'l>, session: &LuaValue, errstr: &'static str, st: Option<stanza::Stanza>, extra: Option<String>) -> LuaResult<()> {
	let cb = match cbtbl.get::<_, Option<LuaFunction>>(IDX_ERROR).unwrap() {
		None => {
			// no error callback, return a lua error
			if let Some(st) = st {
				return Err(LuaError::RuntimeError(format!("XML stream error: {}: {:?}", errstr, st)));
			} else if let Some(extra) = extra {
				return Err(LuaError::RuntimeError(format!("XML stream error: {}: {}", errstr, extra)));
			} else {
				return Err(LuaError::RuntimeError(format!("XML stream error: {}", errstr)));
			}
		},
		Some(cb) => cb,
	};
	match st {
		Some(st) => cb.call::<_, LuaValue>((session.clone(), errstr.to_string(), stanza::lua::LuaStanza::wrap(st)))?,
		None => match Some(extra) {
			Some(extra) => cb.call::<_, LuaValue>((session.clone(), errstr.to_string(), extra))?,
			None => cb.call::<_, LuaValue>((session.clone(), errstr.to_string()))?,
		}
	};
	Ok(())
}

#[derive(Debug, Clone)]
struct ProsodyXmlContext(Rc<rxml::Context>);

impl LuaUserData for ProsodyXmlContext {
	fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
		methods.add_method("release_temporaries", |l: &'lua Lua, this: &ProsodyXmlContext, _: ()| -> LuaResult<()> {
			this.0.release_temporaries();
			Ok(())
		});

		methods.add_meta_method(LuaMetaMethod::ToString, |l: &'lua Lua, this: &ProsodyXmlContext, _: ()| -> LuaResult<String> {
			Ok(format!("{:?}", this.0))
		});

		methods.add_method("get_cdatas", |l: &'lua Lua, this: &ProsodyXmlContext, _: ()| -> LuaResult<usize> {
			Ok(this.0.cdatas())
		});

		methods.add_method("get_cdata_capacity", |l: &'lua Lua, this: &ProsodyXmlContext, _: ()| -> LuaResult<usize> {
			Ok(this.0.cdata_capacity())
		});

		methods.add_method("dump", |l: &'lua Lua, this: &ProsodyXmlContext, _: ()| -> LuaResult<String> {
			Ok(format!("{:X}", *this.0))
		});
	}
}

#[derive(Debug)]
struct ProsodyXmppStream<'x>{
	stream: stream::XMPPStream<'x>,
}

impl<'x> ProsodyXmppStream<'x> {
	fn new_from_streamcallbacks<'l>(tbl: &LuaTable<'l>) -> LuaResult<ProsodyXmppStream<'x>> {
		Ok(ProsodyXmppStream{
			stream: stream::XMPPStream::new(capture_stream_config(tbl)?),
		})
	}
}

impl<'l> LuaUserData for ProsodyXmppStream<'l> {
	fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
		methods.add_function("feed", |l: &'lua Lua, (this, data): (LuaAnyUserData, BString)| -> LuaResult<stream::Result<bool>> {
			{
				let mut stream = this.borrow_mut::<ProsodyXmppStream>()?;
				match stream.stream.feed(data.to_vec()) {
					Err(other) => return Ok(Err(other)),
					Ok(()) => (),
				};
			}

			let cbtbl = this.get_user_value::<LuaTable>().unwrap();
			let session = cbtbl.get::<_, LuaValue>(IDX_SESSION).unwrap();
			loop {
				let mut stream = this.borrow_mut::<ProsodyXmppStream>()?;
				let ev = match stream.stream.read() {
					// not enough data, return true and no error
					Err(stream::Error::ParserError(rxml::Error::IO(ioerr))) => {
						if ioerr.kind() == io::ErrorKind::WouldBlock {
							return Ok(Ok(true))
						} else {
							return Ok(Err(stream::Error::ParserError(rxml::Error::IO(ioerr))))
						}
					},
					Err(stream::Error::InvalidTopLevelElement) => {
						drop(stream);
						cb_error_or_ret(&cbtbl, &session, "invalid-top-level-element", None, None)?;
						return Ok(Ok(true));
					},
					Err(stream::Error::InvalidStreamHeader) => {
						drop(stream);
						cb_error_or_ret(&cbtbl, &session, "no-stream", None, Some("TBD".to_string()))?;
						return Ok(Ok(true));
					},
					Err(other) => return Ok(Err(other)),
					Ok(Some(ev)) => ev,
					Ok(None) => return Ok(Ok(true)),
				};
				match ev {
					stream::Event::Opened{from, to, id, lang, version} => {
						let cb = cbtbl.get::<_, LuaFunction>(IDX_STREAMOPENED).unwrap();
						let attrs = l.create_table()?;
						maybe_set(&attrs, "to", to)?;
						maybe_set(&attrs, "from", from)?;
						maybe_set(&attrs, "id", id)?;
						maybe_set(&attrs, "xml:lang", lang)?;
						maybe_set(&attrs, "version", version)?;
						drop(stream);
						cb.call::<_, LuaValue>((session.clone(), attrs))?;
					},
					stream::Event::Closed => {
						let cb = cbtbl.get::<_, LuaFunction>(IDX_STREAMCLOSED).unwrap();
						drop(stream);
						cb.call::<_, LuaValue>(session.clone())?;
					},
					stream::Event::Stanza(st) => {
						let cb = cbtbl.get::<_, LuaFunction>(IDX_STANZA).unwrap();
						drop(stream);
						cb.call::<_, LuaValue>((session.clone(), stanza::lua::LuaStanza::wrap(st)))?;
					},
					stream::Event::Error(st) => {
						drop(stream);
						cb_error_or_ret(&cbtbl, &session, "stream-error", Some(st), None)?;
					},
				}
			}
		});

		methods.add_method_mut("reset", |l: &'lua Lua, this: &mut ProsodyXmppStream, _: ()| -> LuaResult<()> {
			this.stream = stream::XMPPStream::new(this.stream.cfg().clone());
			Ok(())
		});

		methods.add_method_mut("release_temporaries", |l: &'lua Lua, this: &mut ProsodyXmppStream, _: ()| -> LuaResult<()> {
			this.stream.release_temporaries();
			Ok(())
		});

		methods.add_meta_method(LuaMetaMethod::ToString, |l: &'lua Lua, this: &ProsodyXmppStream, _: ()| -> LuaResult<String> {
			Ok(format!("{:?}", this))
		});
	}
}

pub fn ctx_new<'l>(l: &'l Lua, (): ()) -> LuaResult<LuaValue<'l>> {
	ProsodyXmlContext(Rc::new(rxml::Context::new())).to_lua(l)
}


pub fn stream_new<'l>(l: &'l Lua, (session, tbl, size_limit): (LuaValue, LuaTable<'l>, Option<usize>)) -> LuaResult<LuaValue<'l>> {
	let cbtbl = capture_callbacks(l, &tbl)?;
	cbtbl.raw_set(IDX_SESSION, session)?;
	let mut xs = ProsodyXmppStream::new_from_streamcallbacks(&tbl)?;
	xs.stream.stanza_limit = Some(size_limit.unwrap_or(1024*1024*10));
	let xs = match xs.to_lua(l)? {
		LuaValue::UserData(ud) => ud,
		_ => panic!("unexpected result of to_lua"),
	};
	xs.set_user_value(cbtbl)?;
	xs.to_lua(l)
}
