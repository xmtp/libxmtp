use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{self, Read, Write};

pub struct LengthDelimitedReader<R> {
    inner: R,
}

impl<R: Read> LengthDelimitedReader<R> {
    pub fn new(reader: R) -> Self {
        LengthDelimitedReader { inner: reader }
    }
}

impl<R: Read> Iterator for LengthDelimitedReader<R> {
    type Item = io::Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        // Read the length of the entry (as a little-endian u32)
        let length = match self.inner.read_u32::<LittleEndian>() {
            Ok(len) => len,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return None,
            Err(e) => return Some(Err(e)),
        };

        // Read the next N bytes for the entry
        let mut buffer = vec![0; length as usize];
        match self.inner.read_exact(&mut buffer) {
            Ok(()) => Some(Ok(buffer)),
            Err(e) => Some(Err(e)),
        }
    }
}

pub struct LengthDelimitedWriter<W> {
    inner: W,
}

impl<W: Write> LengthDelimitedWriter<W> {
    pub fn new(writer: W) -> Self {
        LengthDelimitedWriter { inner: writer }
    }

    pub fn write_entry(&mut self, data: &[u8]) -> io::Result<()> {
        if data.len() > u32::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Entry is too long",
            ));
        }

        // Write the length of the entry (as a little-endian u8)
        self.inner.write_u32::<LittleEndian>(data.len() as u32)?;

        // Write the entry data itself
        self.inner.write_all(data)?;

        Ok(())
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_read_write() {
        let tmpfile = NamedTempFile::new().unwrap();
        let mut writer = LengthDelimitedWriter::new(&tmpfile);

        let messages: Vec<Vec<u8>> = vec![
            vec![1, 2, 3],
            vec![4, 5, 6, 7],
            vec![8, 9, 10, 11, 12],
            vec![13],
        ];

        for message in &messages {
            writer.write_entry(message).unwrap();
        }

        let tmp_reader = tmpfile.reopen().unwrap();

        let reader = LengthDelimitedReader::new(&tmp_reader);
        let mut num_items = 0;
        for result in reader {
            num_items += 1;
            let item = result.unwrap();
            assert!(messages.contains(&item));
        }

        assert_eq!(num_items, messages.len());
    }
}
