use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const DEFAULT_PAGE_SIZE: usize = 4096;
const DB_MAGIC: [u8; 4] = *b"AMDB";
const DB_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageId(pub u64);

#[derive(Debug, Clone)]
pub struct Page {
    pub id: PageId,
    pub data: Vec<u8>,
}

impl Page {
    pub fn new(id: PageId, page_size: usize) -> Self {
        Self {
            id,
            data: vec![0u8; page_size],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PageHeader {
    pub magic: [u8; 4],
    pub version: u16,
    pub page_size: u32,
    pub next_page_id: u64,
}

impl PageHeader {
    pub const SIZE: usize = 4 + 2 + 4 + 8;

    pub fn new(page_size: u32, next_page_id: u64) -> Self {
        Self {
            magic: DB_MAGIC,
            version: DB_VERSION,
            page_size,
            next_page_id,
        }
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4..6].copy_from_slice(&self.version.to_le_bytes());
        buf[6..10].copy_from_slice(&self.page_size.to_le_bytes());
        buf[10..18].copy_from_slice(&self.next_page_id.to_le_bytes());
        buf
    }

    pub fn from_bytes(buf: &[u8; Self::SIZE]) -> io::Result<Self> {
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&buf[0..4]);
        let version = u16::from_le_bytes([buf[4], buf[5]]);
        let page_size = u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]);
        let next_page_id = u64::from_le_bytes([
            buf[10], buf[11], buf[12], buf[13], buf[14], buf[15], buf[16], buf[17],
        ]);

        Ok(Self {
            magic,
            version,
            page_size,
            next_page_id,
        })
    }
}

pub struct StorageManager {
    file: File,
    path: PathBuf,
    page_size: usize,
    next_page_id: PageId,
}

impl StorageManager {
    pub fn open(path: impl AsRef<Path>, page_size: usize) -> io::Result<Self> {
        if page_size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "page size must be greater than 0",
            ));
        }
        if page_size > u32::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "page size exceeds u32::MAX and cannot be serialized",
            ));
        }

        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)?;

        let file_len = file.metadata()?.len();
        if file_len == 0 {
            let header = PageHeader::new(page_size as u32, 0);
            file.write_all(&header.to_bytes())?;
            file.flush()?;
            file.sync_data()?;
            return Ok(Self {
                file,
                path,
                page_size,
                next_page_id: PageId(0),
            });
        }

        let mut buf = [0u8; PageHeader::SIZE];
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut buf)?;
        let header = PageHeader::from_bytes(&buf)?;
        if header.magic != DB_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalide datbaase header magic",
            ));
        }
        if header.version != DB_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsuported database version",
            ));
        }
        if header.page_size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid database header page size: 0",
            ));
        }

        Ok(Self {
            file,
            path,
            page_size: header.page_size as usize,
            next_page_id: PageId(header.next_page_id),
        })
    }

    pub fn read_page(&mut self, page_id: PageId) -> io::Result<Page> {
        let mut page = Page::new(page_id, self.page_size);
        let offset = self.page_offset(page_id)?;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(&mut page.data)?;
        Ok(page)
    }

    pub fn write_page(&mut self, page: &Page) -> io::Result<()> {
        if page.data.len() != self.page_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "page size mismatch",
            ));
        }
        if page.id.0 >= self.next_page_id.0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot write to an unallocated page id",
            ));
        }

        let offset = self.page_offset(page.id)?;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(&page.data)?;
        Ok(())
    }

    pub fn allocate_page(&mut self) -> io::Result<PageId> {
        // reserve physical space for page in the file
        let start = self.page_offset(self.next_page_id)?;
        self.file.seek(SeekFrom::Start(start))?;
        self.file.write_all(&vec![0u8; self.page_size])?;

        // Advance the next page id
        let allocated_page = self.next_page_id;
        self.next_page_id = PageId(self.next_page_id.0 + 1);
        
        // persist header after page id has been advanced
        if let Err(e) = self.persist_header() {
            self.next_page_id = allocated_page;
            return Err(e);
        }

        Ok(allocated_page)
    }
    
    pub fn page_size(&self) -> io::Result<usize> {
        Ok(self.page_size)
    }    

    fn persist_header(&mut self) -> io::Result<()> {
        let header = PageHeader::new(self.page_size as u32, self.next_page_id.0 as u64);
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header.to_bytes())?;
        self.file.flush()?;
        self.file.sync_data()?;
        Ok(())
    }

    fn page_offset(&self, page_id: PageId) -> io::Result<u64> {
        let page_size = self.page_size as u64;
        page_id.0
            .checked_mul(page_size)
            .and_then(|offset| offset.checked_add(PageHeader::SIZE as u64))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "page offset calculation overflowed")
            })
    }

        
}
