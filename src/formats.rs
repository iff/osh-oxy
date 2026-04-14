//! Supported history file formats: [`json_lines`] (deprecated) and [`rmp`] (current binary).
pub mod json_lines;
pub mod rmp;

/// all formats we support
pub enum Kind {
    JsonLines,
    Rmp,
}

impl Kind {
    /// get file extension for a format kind
    pub fn extension(&self) -> String {
        match self {
            Kind::JsonLines => "osh".to_string(),
            Kind::Rmp => "bosh".to_string(),
        }
    }
}
