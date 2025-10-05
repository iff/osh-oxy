use pin_project::pin_project;
use serde::Serialize;
use std::io::Result;
use std::marker::Unpin;
use std::pin::Pin;
use tokio::io::{AsyncWrite, AsyncWriteExt};

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
        let mut buf = serde_json::to_vec(value)?;
        buf.push(b'\n');
        self.inner.write_all(&buf).await?;
        Ok(())
    }

    pub async fn flush(&mut self) -> Result<()>
    where
        W: Unpin,
    {
        self.inner.flush().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbitrary::Arbitrary;
    use arbitrary::Unstructured;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn write_binary_event() -> Result<()> {
        let mut temp_file = NamedTempFile::new()?;
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
}
