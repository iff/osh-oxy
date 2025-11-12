use std::{io::Result, marker::Unpin, path::Path};

use pin_project::pin_project;
use rmp_serde::{decode, encode::to_vec};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt, BufReader},
};

use crate::{
    event::{Event, EventFilter, Events},
    formats::EventWriter,
};

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

impl<W: AsyncWrite + Unpin + Send> EventWriter for AsyncBinaryWriter<W> {
    async fn write(&mut self, event: Event) -> anyhow::Result<()> {
        let data = to_vec(&event).expect("encoding value");
        let mut buf = (data.len() as u64).to_le_bytes().to_vec();
        buf.extend(data);
        self.inner.write_all(&buf).await?;
        Ok(())
    }

    async fn flush(&mut self) -> anyhow::Result<()> {
        self.inner.flush().await?;
        Ok(())
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
}

#[allow(dead_code)]
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
    use arbitrary::{Arbitrary, Unstructured};
    use tempfile::NamedTempFile;

    use super::*;

    #[tokio::test]
    async fn write_binary_event() -> anyhow::Result<()> {
        let temp_file = NamedTempFile::new()?;

        let data = &[0; 1000];
        let mut u = Unstructured::new(data);
        let e = crate::event::Event::arbitrary(&mut u).unwrap();

        let mut writer = AsyncBinaryWriter::new(tokio::fs::File::create(temp_file.path()).await?);
        e.write(&mut writer).await?;

        Ok(())
    }

    #[tokio::test]
    async fn roundtrip_binary_event() -> anyhow::Result<()> {
        let num_events = 30;
        let data = &[0; 300];
        let mut u = Unstructured::new(data);

        let mut events = Vec::new();
        let mut buffer = Vec::new();
        let mut writer = AsyncBinaryWriter::new(&mut buffer);

        for _ in 0..num_events {
            let event = crate::event::Event::arbitrary(&mut u).unwrap();
            event.clone().write(&mut writer).await?;
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
