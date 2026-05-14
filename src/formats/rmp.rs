//! Binary format using `rmp_serde`. Wire layout: each record is an 8-byte LE length prefix
//! followed by a msgpack-encoded [`Event`]. This allows O(1) appends without deserialising the
//! whole file.
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

    /// # Errors
    ///
    /// Will return an `Err` if serialisation or writing to file fails.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "contract shoul be that the event is consumed by write"
    )]
    pub fn write(&mut self, event: Event) -> anyhow::Result<()> {
        let data = to_vec(&event)?;
        let mut buf = (data.len() as u64).to_le_bytes().to_vec();
        buf.extend(data);
        self.inner.write_all(&buf)?;
        Ok(())
    }

    /// # Errors
    ///
    /// Will return an `Err` if flushing fails.
    pub fn flush(&mut self) -> anyhow::Result<()> {
        self.inner.flush()?;
        Ok(())
    }
}

/// parse and collect all [`Event`]s in the slice
///
/// # Errors
///
/// Will return an `Err` decoding fails (truncation or invalid format).
pub fn load_osh_events(data: &[u8]) -> std::io::Result<Vec<Event>> {
    let mut events = Vec::new();
    let mut cursor = 0;

    while cursor < data.len() {
        #[expect(clippy::missing_panics_doc, reason = "infallible")]
        #[expect(
            clippy::expect_used,
            reason = "errors if we can't read exactly 8 bytes"
        )]
        let size_bytes: [u8; 8] = data
            .get(cursor..cursor + 8)
            .ok_or(std::io::ErrorKind::UnexpectedEof)?
            .try_into()
            .expect("slice is exactly 8 bytes");
        #[expect(
            clippy::cast_possible_truncation,
            reason = "assuming above write was used"
        )]
        let event_size = u64::from_le_bytes(size_bytes) as usize;
        cursor += 8;

        let event: Event = decode::from_slice(
            data.get(cursor..cursor + event_size)
                .ok_or(std::io::Error::from(std::io::ErrorKind::UnexpectedEof))?,
        )
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

        let data: Vec<u8> = (1u8..=255).cycle().take(3000).collect();
        let mut u = Unstructured::new(&data);
        let e = crate::event::Event::arbitrary(&mut u).unwrap();

        let mut writer = BinaryWriter::new(std::fs::File::create(temp_file.path())?);
        e.write(&mut writer)?;

        Ok(())
    }

    #[test]
    fn roundtrip_binary_event() -> anyhow::Result<()> {
        let num_events = 30;
        let data: Vec<u8> = (1u8..=255).cycle().take(1000).collect();
        let mut u = Unstructured::new(&data);

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
        assert!(read_events.into_iter().eq(events.into_iter()));

        Ok(())
    }
}
