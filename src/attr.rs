use crate::stanza;

#[derive(Clone)]
pub struct AttributePath(stanza::StanzaPath);

impl AttributePath {
	pub fn wrap(p: stanza::StanzaPath) -> AttributePath {
		AttributePath(p)
	}

	pub fn get(&self, k: String) -> Option<String> {
		let el = self.0.deref_as_element()?;
		Some(el.attr.get(&k)?.clone())
	}

	pub fn set(&mut self, k: String, v: String) {
		let mut el = match self.0.deref_as_element_mut() {
			Some(el) => el,
			None => return,
		};
		el.attr.insert(k, v);
	}
}
