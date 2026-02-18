# Milestones

## Phase 1: The Storage Skeleton & Page Layout (Q1)

These tasks focus on getting raw bytes onto the disk in a structured way.

### M1: The File Handshake (Focus: Rust std::fs)

- Task 1.1: Setup struct StorageManager that opens/creates a file with OpenOptions (Read, Write, Create).
- Task 1.2: Write a write_page(page_id, buffer) function using File::seek to find the offset (page_id * 4096).
- Task 1.3: Write a read_page(page_id) function that returns a fixed-size array [u8; 4096].
- Task 1.4: Create a "Smoke Test": Write "Hello" to Page 5, close the program, reopen, and read Page 5 to verify.

### M2: Slotted Pages (Focus: Serialization/Memory)

- Task 2.1: Define struct PageHeader (Checksum, FreeSpacePointer, SlotCount).
- Task 2.2: Implement serialize and deserialize for the Header using the zerocopy or byteorder crate.
- Task 2.3: Implement the "Insert Slot" logic: Move the FreeSpacePointer and write a record's bytes to the end of the page.
- Task 2.4: Write the "Slot Directory" logic: Store the (Offset, Length) of each record at the top of the page.

## Phase 2: The Buffer Pool & WAL (Q1-Q2)

This is where the database moves from "writing to files" to "managing memory."

### M3: The Buffer Pool (Focus: Logic & Interior Mutability)

- Task 3.1: Define struct Frame (contains the 4KB page + is_dirty flag + pin_count).
- Task 3.2: Implement a HashMap<PageId, FrameId> to track which pages are currently in memory.
- Task 3.3: Implement a basic LRU (Least Recently Used) replacement policy.
 Task 3.4: Implement fetch_page(page_id): If it's in the pool, return it; if not, find an empty frame (or evict one), and call StorageManager::read_page.

### M4: Write-Ahead Log (WAL) (Focus: Durability)

- Task 4.1: Define LogRecord types (e.g., INSERT, UPDATE, DELETE, BEGIN, COMMIT).
- Task 4.2: Implement a LogManager that appends these records to a separate .log file.
- Task 4.3: Implement "Log-Ahead" logic: Ensure LogManager.flush() is called before the BufferPool writes a dirty page to the data file.
- Task 4.4: Write a recovery script: Read the .log file from start to finish and "redo" the operations on the data file.
