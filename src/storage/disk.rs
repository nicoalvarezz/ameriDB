use std::fs::{File, OpenOptions};
use std::io::{self};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

pub const PAGE_SIZE: usize = 4096;
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

#[derive(Debug)]
pub struct StorageManager {
    file: File,
    path: PathBuf,
    page_size: usize,
    next_page_id: PageId,
}

impl StorageManager {
    pub fn open(
       data_file_path: impl AsRef<Path>,
       page_size: usize,
    ) -> io::Result<Self> {
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

        let path = data_file_path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)?;

        let file_len = file.metadata()?.len();

        let (page_size, next_page_id) = if file_len == 0 {
            // Brand new database file -> initialise file header
            let header = PageHeader::new(page_size as u32, 0);
            file.write_all_at(&header.to_bytes(), 0)?;
            file.sync_data()?;
            (page_size, PageId(0))
        } else {
            // Existing database file -> read header
            let mut buf = [0u8; PageHeader::SIZE];
            file.read_exact_at(&mut buf, 0)?;

            let header = PageHeader::from_bytes(&buf)?;

            if header.magic != DB_MAGIC {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid datbaase header magic",
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
                    "zero page size in header",
                ));
            }

            (header.page_size as usize, PageId(header.next_page_id))
        };

        Ok(Self {
            file,
            path, 
            page_size,
            next_page_id,
        })
    }

    pub fn read_page(&mut self, page_id: PageId) -> io::Result<Page> {
        let mut page = Page::new(page_id, self.page_size);
        let offset = self.page_offset(page_id)?;
        self.file.read_exact_at(&mut page.data, offset)?;
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
        self.file.write_all_at(&page.data, offset)?;
        Ok(())
    }

    pub fn allocate_page(&mut self) -> io::Result<PageId> {
        // reserve physical space for page in the file
        let start = self.page_offset(self.next_page_id)?;
        self.file.write_all_at(&vec![0u8; self.page_size], start)?;

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

    pub fn sync_data(&self) -> io::Result<()> {
        self.file.sync_data()
    }

    pub fn sync_all(&self) -> io::Result<()> {
        self.file.sync_all()
    }

    pub fn page_size(&self) -> io::Result<usize> {
        Ok(self.page_size)
    }

    fn persist_header(&mut self) -> io::Result<()> {
        let header = PageHeader::new(self.page_size as u32, self.next_page_id.0 as u64);
        self.file.write_all_at(&header.to_bytes(), 0)?;
        Ok(())
    }

    fn page_offset(&self, page_id: PageId) -> io::Result<u64> {
        let page_size = self.page_size as u64;
        page_id
            .0
            .checked_mul(page_size)
            .and_then(|offset| offset.checked_add(PageHeader::SIZE as u64))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "page offset calculation overflowed",
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn storage_manager_new_file_has_valid_header() -> io::Result<()> {
        let temp_dir = tempdir()?;
        let path = temp_dir.path().join("test.db");

        let manager = StorageManager::open(&path, PAGE_SIZE)?;
        
        let mut buf = [0u8; PageHeader::SIZE];
        manager.file.read_exact_at(&mut buf, 0)?;

        let header = PageHeader::from_bytes(&buf)?;

        assert_eq!(header.magic, DB_MAGIC);
        assert_eq!(header.version, DB_VERSION);
        Ok(())
    }

    #[test]
    fn storage_manager_round_trip_single_page() -> io::Result<()> {
        let temp_dir = tempdir()?;
        let path = temp_dir.path().join("test.db");

        let mut manager = StorageManager::open(&path, PAGE_SIZE)?;

        let page_id = manager.allocate_page()?;
        let mut page = Page::new(page_id, PAGE_SIZE);
        page.data[0..5].copy_from_slice(b"hello");

        manager.write_page(&page)?;

        let read_page = manager.read_page(page_id)?;
        assert_eq!(&read_page.data[0..5], b"hello");

        Ok(())
    }

    #[test]
    fn storage_manager_allocates_incremental_next_page_id() -> io::Result<()> {
        let temp_dir= tempdir()?;
        let path = temp_dir.path().join("test.db");

        let mut manager = StorageManager::open(path, PAGE_SIZE)?;

        // after allocating 5 pages (0-5), next_page_id should be 5
        for _ in 0..5 {
            manager.allocate_page()?;
        }
        
        assert_eq!(manager.next_page_id.0, 5);

        Ok(())
    }

    #[test]
    fn storage_manager_data_persist_after_sync_and_reopen() -> io::Result<()> {
        let temp_dir = tempdir()?;
        let path = temp_dir.path().join("test.db");

        let mut manager = StorageManager::open(&path, PAGE_SIZE)?;
        let page_id = manager.allocate_page()?;
        let mut page = Page::new(page_id, PAGE_SIZE);
        page.data[0..5].copy_from_slice(b"hello");

        manager.write_page(&page)?;
        manager.sync_all()?;

        let mut new_manager = StorageManager::open(path, PAGE_SIZE)?;
        let read_page = new_manager.read_page(page_id)?;

        assert_eq!(&read_page.data[0..5], b"hello");

        Ok(())
    }

    #[test]
    fn storage_manager_reopen_recovers_next_page_id() -> io::Result<()> {
        let temp_dir = tempdir()?;
        let path = temp_dir.path().join("test.db");

        {
            let mut manager = StorageManager::open(&path, PAGE_SIZE)?;
            manager.allocate_page()?;
            manager.allocate_page()?;
            manager.allocate_page()?;
            manager.sync_all()?;
        }

        let mut reopened = StorageManager::open(&path, PAGE_SIZE)?;
        let next = reopened.allocate_page()?;

        assert_eq!(next.0, 3);
        assert_eq!(reopened.next_page_id.0, 4);

        Ok(())
    }
    
    #[test]
    fn storage_manager_write_beyond_allocated_fails() -> io::Result<()> {
        let temp_dir = tempdir()?;
        let path = temp_dir.path().join("test.db");

        let mut manager = StorageManager::open(path, PAGE_SIZE)?;
        let page = Page::new(PageId(1), PAGE_SIZE); 

        let err = manager
            .write_page(&page)
            .expect_err("writing beyond allocated page range should fail");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(err.to_string(), "cannot write to an unallocated page id");
        
        Ok(())   
    }

    #[test]
    fn storage_manager_open_fails_on_zero_page_size() -> io::Result<()> {
        let temp_dir = tempdir()?;
        let path = temp_dir.path().join("test.db");

        let err = StorageManager::open(path, 0)
            .expect_err("opening file with 0 page size should fail");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    
        Ok(())
    }

    #[test]
    fn storage_manager_open_fails_on_ivalid_magic() -> io::Result<()> {
        let temp_dir = tempdir()?;
        let path = temp_dir.path().join("test.db"); 
        
        let mut buf = [0u8; PageHeader::SIZE];
        buf[0..4].copy_from_slice(&[0u8; 4]);
        std::fs::File::create(&path)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)?;
        std::fs::File::write_at(&file, &buf, 0)?;

        let err = StorageManager::open(path, PAGE_SIZE)
            .expect_err("opening file with invalid magic should fail");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(err.to_string(), "Invalid datbaase header magic");
        
        Ok(())
    }

    #[test]
    fn storage_manager_open_fails_on_unsupported_version() -> io::Result<()> {
        let tem_dir = tempdir()?;
        let path = tem_dir.path().join("test.db");

        let mut buf = [0u8; PageHeader::SIZE];
        buf[0..4].copy_from_slice(&DB_MAGIC);
        buf[4..6].copy_from_slice(&[0u8; 2]);
        std::fs::File::create(&path)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)?;
        std::fs::File::write_at(&file, &buf, 0)?;

        let err = StorageManager::open(path, PAGE_SIZE)
            .expect_err("opening file with invalid version should fail");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(err.to_string(), "unsuported database version");

        Ok(())
    }

    #[test]
    fn storage_manager_sync_all_updates_file_metadata() -> io::Result<()> {
        let temp_dir = tempdir()?;
        let path = temp_dir.path().join("test.db");
        
        // Fres open - file is small
        let mut manager = StorageManager::open(&path, PAGE_SIZE)?;
        let initial_len = manager.file.metadata()?.len();
        assert!(initial_len <= PageHeader::SIZE as u64);

        // Allocate + write something
        let page_id = manager.allocate_page()?;
        let mut page = manager.read_page(page_id)?;
        page.data[0..5].copy_from_slice(b"hello");
        manager.write_page(&page)?;

        // Check BEFORE sync all
        let len_before_sync = manager.file.metadata()?.len();
        
        assert!(len_before_sync > initial_len);

        // Force metadate + data durable
        manager.sync_all()?;
        
        // Check AFTER sync all - size MUST be updated now
        let len_after_sync = manager.file.metadata()?.len();
        let expected_len = PageHeader::SIZE as u64 + (page_id.0 + 1) * PAGE_SIZE as u64;

        assert!(len_after_sync >= expected_len);

        // Re-open the file - metadata & header must be consistent
        drop(manager);
        let mut reopened = StorageManager::open(path, PAGE_SIZE)?;
        let len_after_reopen = reopened.file.metadata()?.len();

        assert_eq!(len_after_reopen, len_after_sync);

        // Data still readable
        let read_back = reopened.read_page(page_id)?;
        assert_eq!(&read_back.data[0..5], b"hello");

        Ok(())
    }
}
