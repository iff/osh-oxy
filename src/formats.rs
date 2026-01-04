pub mod json_lines;
pub mod rmp;

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
