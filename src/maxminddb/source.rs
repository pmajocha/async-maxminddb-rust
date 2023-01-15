use std::io::SeekFrom;

use tokio::io::{AsyncRead, AsyncSeek, AsyncSeekExt, AsyncReadExt};

use crate::MaxMindDBError;

pub struct Source<S: AsyncRead + AsyncSeek + Unpin> {
    buffer: Vec<u8>,
    stream: S,
    pub total_size: usize,
}

impl Source<tokio::fs::File> {
    pub async fn new(path: &str) -> Result<Source<tokio::fs::File>, MaxMindDBError> {
        let file = tokio::fs::File::open(path).await?;
        Ok(Self {
            buffer: Vec::with_capacity(1024),
            total_size: file.metadata().await?.len() as usize,
            stream: file,
        })
    }
}

impl<S: AsyncSeek + AsyncRead + Unpin> Source<S> {
    /// based on sizes required should adjust the buffer, to keep it as small as possible,
    /// yet not relocate too often. For the experiment will always adjust to so far biggest size
    fn adjust_buffer(&mut self, size: usize) {
        if size > self.buffer.len() {
            self.buffer.resize(size, 0);
        }
    }

    pub async fn position(&mut self) -> Result<u64, MaxMindDBError> {
        Ok(self.stream.stream_position().await?)
    }

    pub async fn move_cursor(&mut self, start: u64) -> Result<u64, MaxMindDBError> {
        Ok(self.stream.seek(SeekFrom::Start(start)).await?)
    }

    pub async fn read(&mut self, size: usize) -> Result<&[u8], MaxMindDBError> {
        self.adjust_buffer(size);
        self.stream.read_exact(&mut self.buffer[..size]).await?;
        Ok(&self.buffer[..size])
    }

    pub async fn read_at(&mut self, start: u64, size: usize) -> Result<&[u8], MaxMindDBError> {
        self.move_cursor(start).await?;
        Ok(self.read(size).await?)
    }

    pub async fn read_one(&mut self, start: u64) -> Result<u8, MaxMindDBError> {
       Ok(self.read_at(start, 1).await?[0]) 
    }
}