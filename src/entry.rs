use super::{Compression, ReadExt, Version};
use byteorder::{ReadBytesExt, LE};
use std::io;

#[derive(Debug)]
pub struct Block {
    pub offset: u64,
    /// size of the compressed block
    pub size: u64,
}

impl Block {
    pub fn new<R: io::Read>(reader: &mut R) -> Result<Self, super::Error> {
        Ok(Self {
            offset: reader.read_u64::<LE>()?,
            size: reader.read_u64::<LE>()?,
        })
    }
}

#[derive(Debug)]
pub struct Entry {
    pub offset: u64,
    pub compressed: u64,
    pub uncompressed: u64,
    pub compression: Compression,
    pub timestamp: Option<u64>,
    pub hash: [u8; 20],
    pub compression_blocks: Option<Vec<Block>>,
    pub encrypted: bool,
    pub block_uncompressed: Option<u32>,
}

impl Entry {
    pub fn new<R: io::Read>(reader: &mut R, version: super::Version) -> Result<Self, super::Error> {
        // since i need the compression flags, i have to store these as variables which is mildly annoying
        let offset = reader.read_u64::<LE>()?;
        let compressed = reader.read_u64::<LE>()?;
        let uncompressed = reader.read_u64::<LE>()?;
        let compression = match reader.read_u32::<LE>()? {
            0x01 | 0x10 | 0x20 => Compression::Zlib,
            _ => Compression::None,
        };
        Ok(Self {
            offset,
            compressed,
            uncompressed,
            compression,
            timestamp: match version == Version::Initial {
                true => Some(reader.read_u64::<LE>()?),
                false => None,
            },
            hash: reader.read_guid()?,
            compression_blocks: match version >= Version::CompressionEncryption
                && compression != Compression::None
            {
                true => Some(reader.read_array(Block::new)?),
                false => None,
            },
            encrypted: version >= Version::CompressionEncryption && reader.read_bool()?,
            block_uncompressed: match version >= Version::CompressionEncryption {
                true => Some(reader.read_u32::<LE>()?),
                false => None,
            },
        })
    }

    pub fn read<R: io::Read + io::Seek>(
        &self,
        reader: &mut R,
        version: super::Version,
        key: Option<&aes::Aes256Dec>,
    ) -> Result<Vec<u8>, super::Error> {
        let mut buf = io::BufWriter::new(Vec::with_capacity(self.uncompressed as usize));
        reader.seek(io::SeekFrom::Start(self.offset))?;
        let mut data = reader.read_len(match self.encrypted {
            // add alignment (aes block size: 16) then zero out alignment bits
            true => (self.compressed + 15) & !17,
            false => self.compressed,
        } as usize)?;
        if self.encrypted {
            if let Some(key) = key {
                use aes::cipher::BlockDecrypt;
                for block in data.chunks_mut(16) {
                    key.decrypt_block(aes::Block::from_mut_slice(block))
                }
                data.truncate(self.compressed as usize);
            }
        }
        use io::Write;
        match self.compression {
            Compression::None => buf.write_all(&data)?,
            Compression::Zlib => todo!(),
            Compression::Gzip => todo!(),
            Compression::Oodle => todo!(),
        }
        buf.flush()?;
        Ok(buf.into_inner()?)
    }
}
