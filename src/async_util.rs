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

        // we can't ask for more data than
        if count > self.buffer.capacity() {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
        }

        // if there's insufficient contiguous space, move any existing content to the beginning
        if self.remaining() < count {
            let available = self.available();
            // moves any data to the front of the buffer
            self.buffer.copy_within(self.begin..self.end, 0);
            self.begin = 0;
            self.end = available;
        }

        while count > self.available() {
            self.read_some().await?;
        }

        let ret = &self.buffer[self.begin .. self.begin + count];
        self.begin += count;
        Ok(ret)
    }

    fn available(&self) -> usize {
        self.end - self.begin
    }

    fn is_empty(&self) -> bool {
        self.begin == self.end
    }

    fn remaining(&self) -> usize { self.buffer.capacity() - self.end }

    async fn read_some(&mut self) -> Result<(), std::io::Error> {
        let count = self.io.read(&mut self.buffer[self.end..]).await?;
        if count == 0 {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
        }
        self.end += count;
        Ok(())
    }

}

#[cfg(test)]
mod test {
    use super::*;
    use tokio_test::block_on;
    use tokio_test::io::Builder;


    #[test]
    fn reads_bytes_from_multiple_os_reads() {
        let mut mock = Builder::new()
            .read(&[0x01, 0x02])
            .read(&[0x03])
            .build();

        let mut reader = BufferedReader::new(10, &mut mock);

        assert_eq!(block_on(reader.read_byte()).unwrap(), 0x01);
        assert_eq!(block_on(reader.read_byte()).unwrap(), 0x02);
        assert_eq!(block_on(reader.read_byte()).unwrap(), 0x03);
        assert!(block_on(reader.read_byte()).is_err());
    }

    #[test]
    fn reads_byte_slice_from_multiple_os_reads() {
        let mut mock = Builder::new()
            .read(&[0x01, 0x02])
            .read(&[0x03])
            .build();

        let mut reader = BufferedReader::new(10, &mut mock);

        assert_eq!(block_on(reader.read_bytes(3)).unwrap(), [0x01, 0x02, 0x03]);
    }

    #[test]
    fn errors_when_more_data_read_than_capacity() {
        let mut mock = Builder::new().build();
        let mut reader = BufferedReader::new(10, &mut mock);
        let result = block_on(reader.read_bytes(11));
        assert_eq!(result.err().unwrap().kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn slides_existing_data_correctly() {
        let mut mock = Builder::new()
            .read(&[0x01, 0x02, 0x03, 0x04])
            .build();


        let mut reader = BufferedReader::new(3, &mut mock);

        assert_eq!(block_on(reader.read_byte()).unwrap(), 0x01);
        // the reader will have to slide the bytes to make space
        assert_eq!(block_on(reader.read_bytes(3)).unwrap(), [0x02, 0x03, 0x04]);
    }
}