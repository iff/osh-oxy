use crate::event::{Event, EventFilter, Events};
use pin_project::pin_project;
use rmp_serde::decode;
use rmp_serde::encode::to_vec;
use serde::Serialize;
use std::io::Result;
use std::marker::Unpin;
use std::path::Path;
use std::pin::Pin;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt};
use tokio::{fs::File, io::BufReader};

#[pin_project]
#[derive(Debug)]
pub struct AsyncBinaryWriter<W: AsyncWrite> {
    #[pin]
    inner: W,
}

impl<W: AsyncWrite> AsyncBinaryWriter<W> {
    pub fn new(writer: W) -> Self {
        AsyncBinaryWriter { inner: writer }
    }
}

impl<W: AsyncWrite> AsyncBinaryWriter<W> {
    pub async fn write<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
        W: Unpin,
    {
        // let mut buf = to_vec(value).expect("encoding value");
        // buf.extend(&(buf.len() as u64).to_le_bytes());
        let data = to_vec(value).expect("encoding value");
        let mut buf = (data.len() as u64).to_le_bytes().to_vec();
        buf.extend(data);
        self.inner.write_all(&buf).await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn flush(&mut self) -> Result<()>
    where
        W: Unpin,
    {
        self.inner.flush().await
    }
}

#[pin_project]
#[derive(Debug)]
pub struct AsyncBinaryReader<R: AsyncRead> {
    #[pin]
    inner: R,
}

impl<R: AsyncRead + AsyncSeek> AsyncBinaryReader<R> {
    #[allow(dead_code)]
    pub fn new(reader: R) -> Self {
        AsyncBinaryReader { inner: reader }
    }

    #[allow(dead_code)]
    pub async fn seek(&mut self, pos: std::io::SeekFrom) -> Result<u64>
    where
        R: Unpin,
    {
        self.inner.seek(pos).await
    }

    #[allow(dead_code)]
    pub async fn read_exact(&mut self, buf: &mut [u8]) -> Result<usize>
    where
        R: Unpin,
    {
        self.inner.read_exact(buf).await
    }

    #[allow(dead_code)]
    pub async fn read_all(&mut self) -> Result<Vec<Event>>
    where
        R: Unpin,
    {
        let mut events = Vec::new();

        loop {
            let mut size_buf = [0u8; 8];
            match self.inner.read_exact(&mut size_buf).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }

            let event_size = u64::from_le_bytes(size_buf) as usize;
            let mut event_buf = vec![0u8; event_size];
            self.inner.read_exact(&mut event_buf).await?;

            let event: Event = decode::from_slice(&event_buf)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            events.push(event);
        }

        Ok(events)
    }

    // TODO this is slow due to all the seeks.. not sure we want to toy with this and just read and
    // reverse after (or memory map the file)
    #[allow(dead_code)]
    pub async fn read_all_backward(&mut self) -> Result<Vec<Event>>
    where
        R: Unpin,
    {
        let mut events = Vec::new();
        let mut current_pos = self.seek(std::io::SeekFrom::End(0)).await?;

        while current_pos > 0 {
            // first read the 8-byte size suffix
            if current_pos < 8 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Stream too short to contain size suffix",
                ));
            }

            self.seek(std::io::SeekFrom::Start(current_pos - 8)).await?;
            let mut size_buf = [0u8; 8];
            self.read_exact(&mut size_buf).await?;
            let event_size = u64::from_le_bytes(size_buf) as usize;

            let event_start = current_pos
                .checked_sub(8 + event_size as u64)
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid event size")
                })?;

            self.seek(std::io::SeekFrom::Start(event_start)).await?;

            let mut event_buf = vec![0u8; event_size];
            self.read_exact(&mut event_buf).await?;
            let event: Event = decode::from_slice(&event_buf)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

            events.push(event);

            current_pos = event_start;
        }

        assert_eq!(current_pos, 0);

        Ok(events)
    }
}

pub async fn load_osh_events(
    osh_file: impl AsRef<Path>,
    filter: &EventFilter,
) -> std::io::Result<Events> {
    let fp = BufReader::new(File::open(osh_file).await?);
    let mut reader = AsyncBinaryReader::new(fp);

    Ok(reader
        .read_all()
        .await?
        .into_iter()
        .filter_map(|event| filter.apply(event))
        .collect::<Events>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbitrary::Arbitrary;
    use arbitrary::Unstructured;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn write_binary_event() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        println!("{:?}", temp_file.path());

        let data = &[0; 1000];
        let mut u = Unstructured::new(data);
        let e = crate::event::Event::arbitrary(&mut u).unwrap();

        let mut buffer = Vec::new();
        AsyncBinaryWriter::new(&mut buffer).write(&e).await?;

        println!("Written bytes: {:?}", buffer);
        println!("Byte count: {}", buffer.len());

        // AsyncBinaryWriter::new(tokio::fs::File::create(temp_file.path()).await?)
        //     .write(&e)
        //     .await?;

        Ok(())
    }

    #[tokio::test]
    async fn roundtrip_binary_event() -> Result<()> {
        let num_events = 30;
        let data = &[0; 300];
        let mut u = Unstructured::new(data);

        let mut events = Vec::new();
        let mut buffer = Vec::new();
        let mut writer = AsyncBinaryWriter::new(&mut buffer);

        for _ in 0..num_events {
            let event = crate::event::Event::arbitrary(&mut u).unwrap();
            writer.write(&event).await?;
            events.push(event);
        }

        let cursor = std::io::Cursor::new(buffer);
        let mut reader = AsyncBinaryReader::new(cursor);
        let read_events = reader.read_all().await?;
        assert_eq!(read_events.len(), num_events);
        assert!(read_events.into_iter().eq(events.into_iter().rev()));

        Ok(())
    }
}
