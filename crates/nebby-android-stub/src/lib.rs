//! Android boundary: SAF document URIs must not be treated as POSIX paths.
use nebby_core::{Config, Title};
#[derive(Clone, Debug)]
pub struct DocumentUri(pub String);
pub trait DocumentAccess {
    fn read_document(&self, uri: &DocumentUri) -> Result<Vec<u8>, String>;
    fn write_document(&self, uri: &DocumentUri, data: &[u8]) -> Result<(), String>;
    fn persist_permission(&self, uri: &DocumentUri) -> Result<(), String>;
}
pub struct AndroidLaunchRequest {
    pub title_id: String,
    pub game: DocumentUri,
    pub profile: String,
    pub native_renderer: bool,
}
pub fn launch_request(c: &Config, t: &Title, game: DocumentUri) -> AndroidLaunchRequest {
    AndroidLaunchRequest {
        title_id: t.id.clone(),
        game,
        profile: c.profile.clone(),
        native_renderer: c.enabled("renderer"),
    }
}
pub trait AndroidRuntime {
    fn start_activity(&self, request: AndroidLaunchRequest) -> Result<(), String>;
    fn stop_runtime(&self) -> Result<(), String>;
}
// No JNI, shell commands, Linux paths or desktop process launching in this crate.
