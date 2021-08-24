use std::fmt;
use std::error;
use std::io;
use std::rc::Rc;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::result::Result as StdResult;
use std::borrow::Cow;
use rxml::EventRead;

use crate::stanza;

pub const XMLNS_STREAMS: &'static str = "http://etherx.jabber.org/streams";

#[derive(Clone, PartialEq, Debug)]
pub enum Error {
	ParserError(rxml::Error),
	// pending, max
	StanzaLimitExceeded,
	// invalid stream
	InvalidStreamHeader,
	// non-stanza or unnamespaced elemet
	InvalidTopLevelElement,
	// non-whitespace text at stanza level
	TextAtStreamLevel,
}

impl fmt::Display for Error {
	fn fmt<'f>(&self, f: &'f mut fmt::Formatter) -> fmt::Result {
		match self {
			Self::ParserError(e) => write!(f, "parser error: {}", e),
			Self::StanzaLimitExceeded => f.write_str("stanza size limit exceeded"),
			Self::InvalidStreamHeader => f.write_str("invalid stream header"),
			Self::InvalidTopLevelElement => f.write_str("invalid top level element"),
			Self::TextAtStreamLevel => f.write_str("text at stream level"),
		}
	}
}

impl From<rxml::Error> for Error {
	fn from(e: rxml::Error) -> Error {
		Error::ParserError(e)
	}
}

impl error::Error for Error {
	fn source(&self) -> Option<&(dyn error::Error + 'static)> {
		match self {
			Self::ParserError(e) => Some(e),
			Self::StanzaLimitExceeded
				| Self::InvalidStreamHeader
				| Self::InvalidTopLevelElement
				| Self::TextAtStreamLevel => None,
		}
	}
}

pub type Result<T> = StdResult<T, Error>;

#[derive(Debug)]
pub enum Event {
	Opened{
		lang: Option<rxml::CData>,
		from: Option<rxml::CData>,
		to: Option<rxml::CData>,
		id: Option<rxml::CData>,
		version: Option<rxml::CData>,
	},
	/// root element
	Stanza(stanza::Stanza),
	/// stream error
	Error(stanza::Stanza),
	/// stream footer
	Closed,
}

enum ProcResult {
	Eof,
	NeedMore,
	Event(Event),
}

#[derive(Clone)]
pub struct StreamConfig {
	pub stream_namespace: Rc<rxml::CData>,
	pub default_namespace: Rc<rxml::CData>,
	pub stream_localname: String,
	pub error_localname: String,
	pub init_ctx: Option<Rc<rxml::Context>>,
}

impl fmt::Debug for StreamConfig {
	fn fmt<'f>(&self, f: &'f mut fmt::Formatter) -> fmt::Result {
		f.debug_struct("StreamConfig")
			.field("stream_namespace", &(&*self.stream_namespace as *const rxml::CData, &*self.stream_namespace))
			.field("default_namespace", &(&*self.default_namespace as *const rxml::CData, &*self.default_namespace))
			.field("stream_localname", &self.stream_localname)
			.field("error_localname", &self.error_localname)
			.finish()
	}
}

impl StreamConfig {
	pub fn c2s() -> StreamConfig {
		StreamConfig{
			stream_namespace: Rc::new(rxml::CData::from_str(XMLNS_STREAMS).unwrap()),
			default_namespace: Rc::new(rxml::CData::from_str("jabber:client").unwrap()),
			stream_localname: "stream".to_string(),
			error_localname: "error".to_string(),
			init_ctx: None,
		}
	}

	pub fn s2s() -> StreamConfig {
		StreamConfig{
			stream_namespace: Rc::new(rxml::CData::from_str(XMLNS_STREAMS).unwrap()),
			default_namespace: Rc::new(rxml::CData::from_str("jabber:server").unwrap()),
			stream_localname: "stream".to_string(),
			error_localname: "error".to_string(),
			init_ctx: None,
		}
	}
}

pub struct XMPPStream<'x> {
	p: rxml::FeedParser<'x>,
	is_open: bool,
	stanza: Option<stanza::Stanza>,
	stanza_size: usize,
	pending_bytes: usize,
	pub stanza_limit: Option<usize>,
	non_streamns_depth: usize,
	cfg: StreamConfig,
	err: Option<Error>,
}

impl fmt::Debug for XMPPStream<'_> {
	fn fmt<'f>(&self, f: &'f mut fmt::Formatter) -> fmt::Result {
		f.debug_struct("XMPPStream")
			.field("is_open", &self.is_open)
			.field("stanza", &self.stanza)
			.field("stanza_size", &self.stanza_size)
			.field("pending_bytes", &self.pending_bytes)
			.field("cfg", &self.cfg)
			.field("err", &self.err)
			.finish()
	}
}

fn convert_attrs(mut rxmlattrs: HashMap<rxml::QName, rxml::CData>) -> HashMap<stanza::AttrName, String> {
	let mut out = HashMap::<stanza::AttrName, String>::new();
	for ((nsuri, localname), value) in rxmlattrs.drain() {
		let key = match nsuri {
			Some(nsuri) if *nsuri == rxml::XMLNS_XML => {
				let mut s = String::with_capacity(localname.len() + 4);
				s.push_str("xml:");
				s.push_str(localname.as_str());
				unsafe { stanza::AttrName::from_string_unsafe(s) }
			},
			Some(nsuri) => stanza::AttrName::compose(Some(&*nsuri), localname),
			None => stanza::AttrName::local_only(localname),
		};
		out.insert(key, value.as_string());
	}
	out
}

fn account_bytes(pending: &mut usize, seen: usize) {
	#[cfg(debug_assertions)]
	{
		*pending = pending.checked_sub(seen).unwrap();
	}
	// if we encounter this in real life, we clamp to zero instead.
	#[cfg(not(debug_assertions))]
	{
		*pending = pending.saturating_sub(seen);
	}
}

fn accum_bytes(accum: &mut usize, add: usize) -> Result<()> {
	*accum = match accum.checked_add(add) {
		Some(v) => v,
		None => return Err(Error::StanzaLimitExceeded),
	};
	Ok(())
}

impl<'x> XMPPStream<'x> {
	// TODO: options!
	pub fn new<'a>(mut cfg: StreamConfig) -> XMPPStream<'a> {
		let mut ctx: Option<Rc<rxml::Context>> = None;
		// no need to preserve a reference to the context in the cfg, too
		std::mem::swap(&mut cfg.init_ctx, &mut ctx);
		XMPPStream{
			p: match ctx {
				Some(ctx) => rxml::FeedParser::with_context(ctx),
				None => rxml::FeedParser::new(),
			},
			is_open: false,
			stanza: None,
			stanza_size: 0,
			pending_bytes: 0,
			stanza_limit: None,
			non_streamns_depth: 0,
			cfg: cfg,
			err: None,
		}
	}

	fn check_poison(&self) -> Result<()> {
		match self.err.as_ref() {
			Some(e) => Err(e.clone()),
			None => Ok(()),
		}
	}

	fn poison(&mut self, e: Error) -> Error {
		self.err = Some(e.clone());
		self.pending_bytes = 0;
		self.p.get_buffer_mut().clear();
		e
	}

	fn proc_event(&mut self, ev: rxml::Event) -> Result<ProcResult> {
		match (ev, self.is_open, self.stanza.as_mut()) {
			// we don’t do anything with the xml declaration
			(rxml::Event::XMLDeclaration(em, ..), _, _) => {
				account_bytes(&mut self.pending_bytes, em.len());
				Ok(ProcResult::NeedMore)
			},
			(rxml::Event::StartElement(em, (nsuri, localname), mut attrs), false, _) => {
				// stream header
				if nsuri.is_none() || nsuri.unwrap().as_str() != *self.cfg.stream_namespace || localname != self.cfg.stream_localname.as_str() {
					return Err(Error::InvalidStreamHeader);
				}
				let mut id: Option<rxml::CData> = None;
				let mut lang: Option<rxml::CData> = None;
				let mut from: Option<rxml::CData> = None;
				let mut to: Option<rxml::CData> = None;
				let mut version: Option<rxml::CData> = None;
				for ((nsuri, localname), value) in attrs.drain() {
					match (nsuri.as_ref().and_then(|x| { Some(x.as_str()) }).unwrap_or(""), localname.as_str()) {
						("", "id") => {
							id = Some(value);
						},
						(ns, "lang") if ns == rxml::XMLNS_XML => {
							lang = Some(value);
						},
						("", "from") => {
							from = Some(value);
						},
						("", "to") => {
							to = Some(value);
						},
						("", "version") => {
							version = Some(value);
						},
						_ => {
							return Err(Error::InvalidStreamHeader)
						}
					}
				}
				self.is_open = true;
				account_bytes(&mut self.pending_bytes, em.len());
				self.p.release_temporaries();
				Ok(ProcResult::Event(Event::Opened{
					lang: lang,
					from: from,
					id: id,
					to: to,
					version: version,
				}))
			},
			(rxml::Event::StartElement(em, (nsuri, localname), attrs), true, st_opt) => {
				let mut converted_attrs = convert_attrs(attrs);
				let nsuri = match nsuri {
					None => None,
					Some(nsuri) => {
						if **nsuri != self.cfg.default_namespace.as_str() || self.non_streamns_depth > 0 {
							self.non_streamns_depth = match self.non_streamns_depth.checked_add(1) {
								None => return Err(Error::ParserError(rxml::Error::RestrictedXml("nested too deep"))),
								Some(v) => v,
							};
							Some(nsuri)
						} else {
							None
						}
					}
				};

				match st_opt {
					Some(st) => {
						accum_bytes(&mut self.stanza_size, em.len())?;
						st.tag(nsuri, localname.into(), Some(converted_attrs));
						Ok(ProcResult::NeedMore)
					},
					None => {
						let stanza = stanza::Stanza::new(
							nsuri,
							localname.into(),
							Some(converted_attrs),
						);
						self.stanza = Some(stanza);
						self.stanza_size = 0;
						accum_bytes(&mut self.stanza_size, em.len())?;
						Ok(ProcResult::NeedMore)
					},
				}
			},
			// no need to check for is_open with text, because the parser will only emit that after the first StartElement
			(rxml::Event::Text(em, cdata), _, None) => {
				account_bytes(&mut self.pending_bytes, em.len());
				// stream-level text; may only be whitespace
				if cdata.split_ascii_whitespace().next().is_some() {
					Err(Error::TextAtStreamLevel)
				} else {
					// make sure to not get excessive memory use from whitespace pings
					self.p.release_temporaries();
					Ok(ProcResult::NeedMore)
				}
			},
			(rxml::Event::Text(em, cdata), _, Some(st)) => {
				accum_bytes(&mut self.stanza_size, em.len())?;
				st.text(cdata.as_string());
				Ok(ProcResult::NeedMore)
			},
			(rxml::Event::EndElement(em), _, None) => {
				account_bytes(&mut self.pending_bytes, em.len());
				Ok(ProcResult::Event(Event::Closed))
			},
			(rxml::Event::EndElement(em), _, Some(st)) => {
				accum_bytes(&mut self.stanza_size, em.len())?;
				self.non_streamns_depth = self.non_streamns_depth.saturating_sub(1);
				if st.is_at_top() {
					// end of stanza \o/
					let mut swap: Option<stanza::Stanza> = None;
					std::mem::swap(&mut swap, &mut self.stanza);
					account_bytes(&mut self.pending_bytes, self.stanza_size);

					let swap = swap.unwrap();
					let root = swap.root();
					let ev = if root.localname == self.cfg.error_localname.as_str() && root.nsuri.as_ref().and_then(|v| { Some(**v == self.cfg.stream_namespace.as_str()) }).unwrap_or(false) {
						drop(root);
						ProcResult::Event(Event::Error(swap))
					} else {
						drop(root);
						ProcResult::Event(Event::Stanza(swap))
					};
					self.p.release_temporaries();
					Ok(ev)
				} else {
					st.up();
					Ok(ProcResult::NeedMore)
				}
			},
		}
	}

	pub fn pending_bytes(&self) -> usize {
		self.pending_bytes
	}

	pub fn feed<T: Into<Cow<'x, [u8]>>>(&mut self, data: T) -> Result<()> {
		self.check_poison()?;
		let data = data.into();
		self.pending_bytes = self.pending_bytes.checked_add(
			data.len()
		).ok_or_else(|| { Error::StanzaLimitExceeded })?;
		match self.stanza_limit {
			Some(v) if v < self.pending_bytes => return Err(Error::StanzaLimitExceeded),
			_ => (),
		}
		self.p.feed(data);
		Ok(())
	}

	pub fn feed_eof(&mut self) {
		self.p.feed_eof();
	}

	pub fn read(&mut self) -> Result<Option<Event>> {
		self.check_poison()?;
		loop {
			let ev = self.p.read();
			match ev {
				Err(rxml::Error::IO(ioerr)) if ioerr.kind() == io::ErrorKind::WouldBlock => {
					if let Some(limit) = self.stanza_limit {
						if self.pending_bytes >= limit {
							return Err(self.poison(Error::StanzaLimitExceeded));
						}
					}
					return Err(Error::ParserError(rxml::Error::IO(ioerr)))
				},
				// all errors which are not I/O errors should immediately poison the stream
				Err(other) => return Err(self.poison(Error::ParserError(other))),
				Ok(Some(ev)) => match self.proc_event(ev)? {
					ProcResult::Eof => return Ok(None),
					ProcResult::NeedMore => continue,
					ProcResult::Event(ev) => return Ok(Some(ev)),
				},
				Ok(None) => return Ok(None),
			}
		}
	}

	pub fn cfg(&self) -> &StreamConfig {
		&self.cfg
	}

	pub fn release_temporaries(&mut self) {
		self.p.release_temporaries()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io;

	#[test]
	fn xmppstream_read_stream_header() {
		let data = &b"<?xml version='1.0'?><stream:stream id='foobar2342' from='capulet.example' to='montague.example' xml:lang='en' xmlns:stream='http://etherx.jabber.org/streams' xmlns='jabber:server' version='1.0'><iq/>"[..];
		let mut s = XMPPStream::new(StreamConfig::s2s());
		s.feed(data).unwrap();
		assert_eq!(s.pending_bytes, data.len());
		let ev = s.read().unwrap().unwrap();
		assert_eq!(s.pending_bytes, 5);
		match ev {
			Event::Opened{id, from, to, lang, version} => {
				assert_eq!(id.unwrap(), "foobar2342");
				assert_eq!(from.unwrap(), "capulet.example");
				assert_eq!(to.unwrap(), "montague.example");
				assert_eq!(lang.unwrap(), "en");
				assert_eq!(version.unwrap(), "1.0");
			},
			other => panic!("unexpected event: {:?}", other),
		}
	}

	#[test]
	fn xmppstream_returns_wouldblock_on_insufficient_data() {
		let data = &b"<?xml version='1.0'?><stream:stream id='foobar2342' from='capulet.example' to='montague.example' xml:lang='en' xmlns:stream='http://etherx.jabber.org/streams' xmlns='jabber:server'"[..];
		let mut s = XMPPStream::new(StreamConfig::s2s());
		s.feed(data).unwrap();
		match s.read() {
			Err(Error::ParserError(rxml::Error::IO(ioerr))) => {
				assert_eq!(ioerr.kind(), io::ErrorKind::WouldBlock);
			},
			other => panic!("unexpected result: {:?}", other),
		}
	}

	#[test]
	fn xmppstream_read_stanza_with_attributes_and_child() {
		let data = &b"<?xml version='1.0'?><stream:stream xmlns:stream='http://etherx.jabber.org/streams' xmlns='jabber:server' version='1.0'><iq id='foobar2342' from='juliet@capulet.example' to='romeo@montague.example' type='get'><query xmlns='http://jabber.org/protocol/disco#info'/></iq>"[..];
		let mut s = XMPPStream::new(StreamConfig::s2s());
		s.feed(data).unwrap();
		assert_eq!(s.pending_bytes, data.len());
		s.read().unwrap().unwrap();
		assert_eq!(s.pending_bytes, 148);
		let ev = s.read().unwrap().unwrap();
		assert_eq!(s.pending_bytes, 0);
		match ev {
			Event::Stanza(st) => {
				println!("stanza: {:?}", st);
				let root = st.root();
				assert_eq!(root.localname, "iq");
				assert_eq!(root.attr.get("id").unwrap(), "foobar2342");
				assert_eq!(root.attr.get("from").unwrap(), "juliet@capulet.example");
				assert_eq!(root.attr.get("to").unwrap(), "romeo@montague.example");
				assert_eq!(root.attr.get("type").unwrap(), "get");
				// we want the xmlns on the stanza to be None to avoid routing to other domains becoming problematic
				assert!(root.attr.get("xmlns").is_none());

				let q_ptr = root[0].as_element_ptr().unwrap();
				let q = q_ptr.borrow();
				assert_eq!(**q.nsuri.as_ref().unwrap(), "http://jabber.org/protocol/disco#info");
				assert_eq!(q.localname, "query");
				assert_eq!(q.attr.len(), 0);
			},
			other => panic!("unexpected event: {:?}", other),
		}
	}

	#[test]
	fn xmppstream_read_stanza_with_attributes_and_child_and_text() {
		let data = &b"<?xml version='1.0'?><stream:stream xmlns:stream='http://etherx.jabber.org/streams' xmlns='jabber:server' version='1.0'><message id='foobar2342' from='juliet@capulet.example' to='romeo@montague.example'><body>oh romeo I don\xe2\x80\x99t know my lines!</body></message>"[..];
		let mut s = XMPPStream::new(StreamConfig::s2s());
		s.feed(data).unwrap();
		s.read().unwrap().unwrap();
		let ev = s.read().unwrap().unwrap();
		match ev {
			Event::Stanza(st) => {
				println!("stanza: {:?}", st);
				let root = st.root();
				assert_eq!(root.localname, "message");
				assert_eq!(root.attr.get("id").unwrap(), "foobar2342");
				assert_eq!(root.attr.get("from").unwrap(), "juliet@capulet.example");
				assert_eq!(root.attr.get("to").unwrap(), "romeo@montague.example");

				let body_ptr = root[0].as_element_ptr().unwrap();
				let body = body_ptr.borrow();
				assert_eq!(body.localname, "body");
				// we want the xmlns on the stanza and its direct same-namespace children to be None
				assert!(body.attr.get("xmlns").is_none());
				assert_eq!(body.get_text().unwrap(), "oh romeo I don’t know my lines!");
			},
			other => panic!("unexpected event: {:?}", other),
		}
	}

	#[test]
	fn xmppstream_read_stanza_sets_stream_xmlns_on_children_below_foreign_ns() {
		let data = &b"<?xml version='1.0'?><stream:stream xmlns:stream='http://etherx.jabber.org/streams' xmlns='jabber:client' version='1.0'><message id='foobar2342' from='juliet@capulet.example' to='romeo@montague.example'><forwarded xmlns='carbons'><message xmlns='jabber:client'></message></forwarded></message>"[..];
		let mut s = XMPPStream::new(StreamConfig::c2s());
		s.feed(data).unwrap();
		s.read().unwrap().unwrap();
		let ev = s.read().unwrap().unwrap();
		match ev {
			Event::Stanza(st) => {
				println!("stanza: {:?}", st);
				let root = st.root();
				assert_eq!(root.localname, "message");
				assert_eq!(root.attr.get("id").unwrap(), "foobar2342");
				assert_eq!(root.attr.get("from").unwrap(), "juliet@capulet.example");
				assert_eq!(root.attr.get("to").unwrap(), "romeo@montague.example");

				let fwd_ptr = root[0].as_element_ptr().unwrap();
				let msg_ptr = fwd_ptr.borrow()[0].as_element_ptr().unwrap();
				let msg = msg_ptr.borrow();
				// we want the xmlns on the stanza and its direct same-namespace children to be None
				assert_eq!(**msg.nsuri.as_ref().unwrap(), "jabber:client");
				assert_eq!(msg.localname, "message");
				assert_eq!(msg.attr.len(), 0);
			},
			other => panic!("unexpected event: {:?}", other),
		}
	}

	#[test]
	fn xmppstream_footer() {
		let data = &b"<?xml version='1.0'?><stream:stream xmlns:stream='http://etherx.jabber.org/streams' xmlns='jabber:client' version='1.0'></stream:stream>"[..];
		let mut s = XMPPStream::new(StreamConfig::c2s());
		s.feed(data).unwrap();
		s.read().unwrap().unwrap();
		let ev = s.read().unwrap().unwrap();
		match ev {
			Event::Closed => (),
			other => panic!("unexpected event: {:?}", other),
		}
	}

	#[test]
	fn xmppstream_maps_xml_attributes() {
		// explicit compat with what prosody does
		let data = &b"<?xml version='1.0'?><stream:stream xmlns:stream='http://etherx.jabber.org/streams' xmlns='jabber:client' version='1.0'><iq xml:lang='en'/>"[..];
		let mut s = XMPPStream::new(StreamConfig::c2s());
		s.feed(data).unwrap();
		s.read().unwrap().unwrap();
		let ev = s.read().unwrap().unwrap();
		match ev {
			Event::Stanza(st) => {
				println!("stanza: {:?}", st);
				let root = st.root();
				assert_eq!(root.localname, "iq");
				assert_eq!(root.attr.get("xml:lang").unwrap(), "en");
				assert!(root.attr.get("http://www.w3.org/XML/1998/namespace\x01lang").is_none());
			},
			other => panic!("unexpected event: {:?}", other),
		}
	}

	#[test]
	fn xmppstream_accepts_and_ignores_stream_level_whitespace() {
		let data = &b"<?xml version='1.0'?><stream:stream xmlns:stream='http://etherx.jabber.org/streams' xmlns='jabber:client' version='1.0'>    <iq/>"[..];
		let mut s = XMPPStream::new(StreamConfig::c2s());
		s.feed(data).unwrap();
		s.read().unwrap().unwrap();
		let ev = s.read().unwrap().unwrap();
		match ev {
			Event::Stanza(st) => {
				println!("stanza: {:?}", st);
				let root = st.root();
				assert_eq!(root.localname, "iq");
			},
			other => panic!("unexpected event: {:?}", other),
		}
	}

	#[test]
	fn xmppstream_rejects_stream_level_text() {
		let data = &b"<?xml version='1.0'?><stream:stream xmlns:stream='http://etherx.jabber.org/streams' xmlns='jabber:client' version='1.0'>    foobar<iq/>"[..];
		let mut s = XMPPStream::new(StreamConfig::c2s());
		s.feed(data).unwrap();
		s.read().unwrap().unwrap();
		let e = s.read().err().unwrap();
		match e {
			Error::TextAtStreamLevel => (),
			other => panic!("unexpected error: {:?}", other),
		}
	}

	#[test]
	fn xmppstream_enforces_stanza_size_limit() {
		let data = &b"<?xml version='1.0'?><stream:stream xmlns:stream='http://etherx.jabber.org/streams' xmlns='jabber:client' version='1.0'><iq id='foobar2342' from='juliet@capulet.example' to='romeo@montague.example' type='get'>"[..];
		let mut s = XMPPStream::new(StreamConfig::c2s());
		s.feed(data).unwrap();
		s.read().unwrap().unwrap();
		s.stanza_limit = Some(32);
		let e = s.read().err().unwrap();
		match e {
			Error::StanzaLimitExceeded => (),
			other => panic!("unexpected error: {:?}", other),
		}
	}

	#[test]
	fn xmppstream_rejects_additional_data_or_read_after_poison_error() {
		let data = &b"<?xml version='1.0'?><stream:stream xmlns:stream='http://etherx.jabber.org/streams' xmlns='jabber:client' version='1.0'><iq id='foobar2342' from='juliet@capulet.example' to='romeo@montague.example' type='get'>"[..];
		let mut s = XMPPStream::new(StreamConfig::c2s());
		s.feed(data).unwrap();
		s.read().unwrap().unwrap();
		s.stanza_limit = Some(32);
		s.read().err().unwrap();
		let e = s.feed(&b"</iq>"[..]).err().unwrap();
		match e {
			Error::StanzaLimitExceeded => (),
			other => panic!("unexpected error: {:?}", other),
		}
		let e = s.read().err().unwrap();
		match e {
			Error::StanzaLimitExceeded => (),
			other => panic!("unexpected error: {:?}", other),
		}
	}

	#[test]
	fn xmppstream_rejects_additional_data_after_xml_error() {
		let data = &b"<?xml version='1.0'?><stream:streamxmlns:stream='http://etherx.jabber.org/streams' xmlns='jabber:client' version='1.0'><iq id='foobar2342' from='juliet@capulet.example' to='romeo@montague.example' type='get'>"[..];
		let mut s = XMPPStream::new(StreamConfig::c2s());
		s.feed(data).unwrap();
		let e = s.read().err().unwrap();
		match e {
			Error::ParserError(_) => (),
			other => panic!("unexpected error: {:?}", other),
		}
		let e = s.feed(&b"</iq>"[..]).err().unwrap();
		match e {
			Error::ParserError(_) => (),
			other => panic!("unexpected error: {:?}", other),
		}
	}

	#[test]
	fn xmppstream_drops_buffered_data_after_xml_error() {
		let data = &b"<?xml version='1.0'?><stream:streamxmlns:stream='http://etherx.jabber.org/streams' xmlns='jabber:client'><iq id='foobar2342' from='juliet@capulet.example' to='romeo@montague.example' type='get'>"[..];
		let mut s = XMPPStream::new(StreamConfig::c2s());
		s.feed(data).unwrap();
		s.read().err().unwrap();
		assert_eq!(s.pending_bytes(), 0);
		s.feed(&b"foo"[..]).err().unwrap();
		assert_eq!(s.pending_bytes(), 0);
	}

	#[test]
	fn xmppstream_read_stream_error() {
		let data = &b"<?xml version='1.0'?><stream:stream xmlns:stream='http://etherx.jabber.org/streams' xmlns='jabber:server' version='1.0'><stream:error><not-well-formed xmlns='urn:ietf:params:xml:ns:xmpp-streams'/></stream:error>"[..];
		let mut s = XMPPStream::new(StreamConfig::s2s());
		s.feed(data).unwrap();
		assert_eq!(s.pending_bytes, data.len());
		s.read().unwrap().unwrap();
		let ev = s.read().unwrap().unwrap();
		match ev {
			Event::Error(st) => {
				println!("stream error: {:?}", st);
				let root = st.root();
				assert_eq!(**root.nsuri.as_ref().unwrap(), XMLNS_STREAMS);
				assert_eq!(root.localname, "error");

				let e_ptr = root[0].as_element_ptr().unwrap();
				let e = e_ptr.borrow();
				assert_eq!(**e.nsuri.as_ref().unwrap(), "urn:ietf:params:xml:ns:xmpp-streams");
				assert_eq!(e.localname, "not-well-formed");
			},
			other => panic!("unexpected event: {:?}", other),
		}
	}

	#[test]
	fn xmppstream_read_stanza_with_complex_namespacing() {
		let data = &b"<?xml version='1.0'?><stream:stream xmlns:stream='streamns' xmlns='stanzans'><x xmlns:a='b'><y xmlns:a='c'><a:z/></y><a:z/></x>"[..];
		let mut s = XMPPStream::new(StreamConfig{
			default_namespace: Rc::new(rxml::CData::from_string("stanzans".to_string()).unwrap()),
			stream_namespace: Rc::new(rxml::CData::from_string("streamns".to_string()).unwrap()),
			stream_localname: "stream".to_string(),
			error_localname: "error".to_string(),
			init_ctx: None,
		});
		s.feed(data).unwrap();
		assert_eq!(s.pending_bytes, data.len());
		s.read().unwrap().unwrap();
		let ev = s.read().unwrap().unwrap();
		match ev {
			Event::Stanza(st) => {
				println!("stanza: {:?}", st);
				let root = st.root();
				assert_eq!(root.localname, "x");
				assert!(root.nsuri.is_none());

				let c_ptr = root[0].as_element_ptr().unwrap();
				let c = c_ptr.borrow();
				assert!(c.nsuri.is_none());
				assert_eq!(c.localname, "y");

				{
					let cc_ptr = c[0].as_element_ptr().unwrap();
					let cc = cc_ptr.borrow();
					assert_eq!(**cc.nsuri.as_ref().unwrap(), "c");
					assert_eq!(cc.localname, "z");
				}

				let c_ptr = root[1].as_element_ptr().unwrap();
				let c = c_ptr.borrow();
				assert_eq!(**c.nsuri.as_ref().unwrap(), "b");
				assert_eq!(c.localname, "z");
			},
			other => panic!("unexpected event: {:?}", other),
		}
	}
}
