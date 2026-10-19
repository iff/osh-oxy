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
    use proptest::{arbitrary::any, strategy::Strategy};

    use super::*;

    fn event() -> impl Strategy<Value = Event> {
        (
            any::<i64>(),
            any::<String>(),
            any::<i64>(),
            any::<i16>(),
            any::<String>(),
            any::<String>(),
            any::<String>(),
        )
            .prop_map(
                |(timestamp_millis, command, endtime, exit_code, folder, machine, session)| Event {
                    timestamp_millis,
                    command,
                    endtime,
                    exit_code,
                    folder,
                    machine,
                    session,
                },
            )
    }

    proptest::proptest! {
        #[test]
        fn event_serde_roundtrip(events in proptest::collection::vec(event(), 0..20)) {
            let mut buffer = Vec::new();
            let mut writer = BinaryWriter::new(&mut buffer);
            for event in events.clone() {
                event.write(&mut writer).unwrap();
            }
            proptest::prop_assert_eq!(load_osh_events(&buffer).unwrap(), events);
        }
    }
}
