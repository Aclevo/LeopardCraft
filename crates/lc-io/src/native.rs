//! Native `.lcscene` wrappers.

use lc_scene::Document;
use std::path::Path;

pub fn save(path: impl AsRef<Path>, doc: &Document) -> Result<(), String> {
    lc_scene::save_native(path, doc)
}

pub fn load(path: impl AsRef<Path>) -> Result<Document, String> {
    lc_scene::load_native(path)
}

pub fn to_json(doc: &Document) -> Result<String, String> {
    lc_scene::to_json(doc)
}

pub fn from_json(text: &str) -> Result<Document, String> {
    lc_scene::from_json(text)
}
