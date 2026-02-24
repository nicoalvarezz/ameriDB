mod storage;

use crate::storage::disk::{DEFAULT_PAGE_SIZE, Page, StorageManager};
use std::io;

fn main() -> io::Result<()> {
    let mut disk = StorageManager::open("data.db", DEFAULT_PAGE_SIZE)?;

    let page_id = disk.allocate_page()?;
    let mut page = Page::new(page_id, disk.page_size().unwrap());
    page.data[0..5].copy_from_slice(b"hello");
    
    disk.write_page(&page)?;
    
    let read_back = disk.read_page(page_id)?;
    assert_eq!(&read_back.data[0..5], b"hello");
    Ok(())
}

