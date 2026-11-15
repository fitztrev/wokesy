use std::io;
use std::sync::{Arc, Mutex};

use tracing::subscriber::DefaultGuard;
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Debug, Default)]
pub struct Logs {
    written: Arc<Mutex<Vec<u8>>>,
}

impl Logs {
    pub fn listen() -> (Self, DefaultGuard) {
        let logs = Self::default();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(logs.clone())
            .finish();
        let listening = tracing::subscriber::set_default(subscriber);
        (logs, listening)
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.written.lock().unwrap()).into_owned()
    }
}

impl io::Write for Logs {
    fn write(&mut self, line: &[u8]) -> io::Result<usize> {
        self.written.lock().unwrap().extend_from_slice(line);
        Ok(line.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl MakeWriter<'_> for Logs {
    type Writer = Self;

    fn make_writer(&self) -> Self {
        self.clone()
    }
}
