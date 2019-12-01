use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;

pub struct BufferedReader<'a, T>  {
    buffer: Vec<u8>,
    io : &'a mut T,
    begin: usize,    // index to the first unread element
    end: usize       // index just past the last unread element
}

impl<'a, T : AsyncRead + std::marker::Unpin> BufferedReader<'a, T> {

    pub fn new(size: usize, io: &'a mut T) -> BufferedReader<T> {
        BufferedReader {
            buffer: vec![0; size],
            io,
            begin: 0,
            end: 0
        }
    }

    fn len(&self) -> usize {
        self.end - self.begin
    }

    fn is_empty(&self) -> bool {
        self.begin == self.end
    }

    pub async fn read_byte(&mut self) -> Result<u8, std::io::Error> {
        if self.is_empty() {
            self.begin = 0;
            self.end = 0;
            self.read_some().await?
        }

        let ret = self.buffer[self.begin];
        self.begin += 1;
        Ok(ret)
    }

    pub async fn read_bytes(&mut self, count: usize) -> Result<&[u8], std::io::Error> {

        if count > self.buffer.capacity() {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
        }

        // we may need to move the existing contents
        // TODO - skip this is there is sufficient space
        if self.begin > 0 {
            let length = self.len();
            // moves any data to the front of the buffer
            self.buffer.copy_within(self.begin..self.end, 0);
            self.begin = 0;
            self.end = length;
        }

        while count > self.len() {
            self.read_some().await?;
        }

        let ret = &self.buffer[self.begin .. self.begin + count];
        self.begin += count;
        Ok(ret)
    }

    async fn read_some(&mut self) -> Result<(), std::io::Error> {
        self.end += self.io.read(&mut self.buffer[self.end..]).await?;
        Ok(())
    }

}