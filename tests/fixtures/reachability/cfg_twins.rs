//! Platform-gated twins: both declarations exist in the file and only one
//! compiles, so a syntax index sees two declarations sharing one name.

#[cfg(unix)]
pub fn platform_root() -> &'static str {
    "/"
}

#[cfg(windows)]
pub fn platform_root() -> &'static str {
    "C:\\"
}

pub struct Client;

impl Client {
    pub fn send(&self) {}
}

pub trait Transport {
    fn send(&self);
}

impl Transport for Client {
    fn send(&self) {}
}
