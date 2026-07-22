pub const DEFAULT_LIMIT: usize = 10;

pub struct Client {
    endpoint: String,
}

impl Client {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }

    /// Sends one payload.
    #[must_use]
    pub fn send(&self, payload: &str) -> usize {
        self.endpoint.len() + payload.len()
    }
}

pub trait Transport {
    type Output;

    fn send(&self, payload: &str) -> Self::Output;
}

impl Transport for Client {
    type Output = usize;

    fn send(&self, payload: &str) -> Self::Output {
        Client::send(self, payload)
    }
}

pub enum State {
    Ready,
    Failed(String),
}

pub mod nested {
    pub fn helper() -> bool {
        true
    }
}

macro_rules! traced {
    ($value:expr) => { $value };
}

pub fn connect() -> Client {
    Client::new(String::new())
}

fn normalized_size(payload: &str) -> usize {
    payload.trim().len()
}

pub fn bounded_size(payload: &str) -> usize {
    normalized_size(payload).min(DEFAULT_LIMIT)
}
