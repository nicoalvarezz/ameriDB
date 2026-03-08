mod storage;

use crate::storage::{database_cluser::DatabaseCluster, database_cluser::DatabaseId, disk::{PAGE_SIZE, Page, StorageManager}};
use std::{io, sync::MutexGuard};

fn main() -> io::Result<()> {
    let cluster = DatabaseCluster::new("./data", PAGE_SIZE)?;
    
    let db_16384 = cluster.open_database(DatabaseId(16384))?;

    {
        let mgr = db_16384.lock().unwrap();
        simple_write(mgr, "hello")?;
    }
    
    let db_1234 = cluster.open_database(DatabaseId(1234))?;
    
    {
        let mgs  = db_1234.lock().unwrap();
        simple_write(mgs, "world")?;
    }

    Ok(())
}

fn simple_write(mut manager: MutexGuard<'_, StorageManager>, content: &str) -> io::Result<()> {
    let page_id = manager.allocate_page()?;
    let mut page = Page::new(page_id, manager.page_size()?);
    page.data[0..5].copy_from_slice(content.as_bytes());
    manager.write_page(&page)?;
    manager.sync_data()?;
    Ok(())
}

