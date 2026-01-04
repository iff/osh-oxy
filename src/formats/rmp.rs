use std::io::Write;

use rmp_serde::{decode, encode::to_vec};

use crate::event::Event;

#[derive(Debug)]
pub struct BinaryWriter<W: Write> {
    inner: W,
}

impl<W: Write> BinaryWriter<W> {
    pub fn new(writer: W) -> Self {
        BinaryWriter { inner: writer }
    }

    pub fn write(&mut self, event: Event) -> anyhow::Result<()> {
        let data = to_vec(&event)?;
        let mut buf = (data.len() as u64).to_le_bytes().to_vec();
        buf.extend(data);
        self.inner.write_all(&buf)?;
        Ok(())
    }

    #[allow(dead_code)]
    fn flush(&mut self) -> anyhow::Result<()> {
        self.inner.flush()?;
        Ok(())
    }
}

pub fn load_osh_events(data: &[u8]) -> std::io::Result<Vec<Event>> {
    let mut events = Vec::new();
    let mut cursor = 0;

    while cursor < data.len() {
        #[allow(clippy::indexing_slicing, clippy::expect_used)]
        let size_bytes: [u8; 8] = data[cursor..cursor + 8]
            .try_into()
            .expect("8 bytes for length encoding");
        let event_size = u64::from_le_bytes(size_bytes) as usize;
        cursor += 8;

        #[allow(clippy::indexing_slicing)]
        let event: Event = decode::from_slice(&data[cursor..cursor + event_size])
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        events.push(event);
        cursor += event_size;
    }

    Ok(events)
}

#[cfg(test)]
mod tests {
    use arbitrary::{Arbitrary, Unstructured};
    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn write_binary_event() -> anyhow::Result<()> {
        let temp_file = NamedTempFile::new()?;

        let data = &[0; 1000];
        let mut u = Unstructured::new(data);
        let e = crate::event::Event::arbitrary(&mut u).unwrap();

        let mut writer = BinaryWriter::new(std::fs::File::create(temp_file.path())?);
        e.write(&mut writer)?;

        Ok(())
    }

    #[test]
    fn roundtrip_binary_event() -> anyhow::Result<()> {
        let num_events = 30;
        let data = &[0; 300];
        let mut u = Unstructured::new(data);

        let mut events = Vec::new();
        let mut buffer = Vec::new();
        let mut writer = BinaryWriter::new(&mut buffer);

        for _ in 0..num_events {
            let event = crate::event::Event::arbitrary(&mut u).unwrap();
            event.clone().write(&mut writer)?;
            events.push(event);
        }

        let read_events = load_osh_events(buffer.as_ref())?;
        assert_eq!(read_events.len(), num_events);
        assert!(read_events.into_iter().eq(events.into_iter().rev()));

        Ok(())
    }
}
