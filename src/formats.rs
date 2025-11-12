pub mod json_lines;
pub mod rmp;

use crate::event::Event;

#[allow(async_fn_in_trait)]
pub trait EventWriter {
    async fn write(&mut self, event: Event) -> anyhow::Result<()>;
    async fn flush(&mut self) -> anyhow::Result<()>;
}

pub enum Kind {
    JsonLines,
    Rmp,
}

impl Kind {
    pub fn extension(&self) -> String {
        match self {
            Kind::JsonLines => "osh".to_string(),
            Kind::Rmp => "bosh".to_string(),
        }
    }
}
