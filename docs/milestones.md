# Milestones

## Phase 1: The Storage Skeleton & Page Layout (Q1)

These tasks focus on getting raw bytes onto the disk in a structured way.

### M1: Storage Skeleton (2 weeks)

Establish the relationship between the rust runtime and the disk.
**Goal**: Create `DiskManager` that handles raw [u8; 4096] pages.
**Artifacts**: ADR "Page Size & Header layout; File format doc v0"

- Task 1.1: Define constants: `PAGE_SIZE = 4096`, `INVALID_PAGE_ID = u32::MAX`.
- Task 1.2: Implement `DiskManager` stuct using `std::fs::File`.
- Task 1.3L Use `File::seek` and `ReadAt`/`WriteAt` (via `std::os::unix::fs::FileExt` on Linux/Mac) to implement
`read_page(page_id)` and `write_page(page_id)`.
- Task 1.4: Define `PageHeader` struct (Magic number, Version, Page Type, Checksum).
- Task 1.5: Write a unit test that writes 10 random pages and reads them back to verify integrity.

### M2: Page Layout (Slotted Page) (2-3 weeks)

Moving from raw bytes to structure data that handles variable-length key-values.

**Goal**: Implement `slottedPage` struct to manage records within a single page
**Artifacts**: ADR "Slotted Pages"

- Task 2.1: Implement a Slot struct (offset, length).
- Task 2.2: Write the SlottedPage view: Header at the front, Slots growing forward, Data growing backward from the end.
- Task 2.3: Implement insert_record(&[u8]) -> returns SlotId.
- Task 2.4: Implement compact(): when a record is deleted, shift remaining data to reclaim contiguous space.
- Task 2.5: Integrate crc32fast to compute checksums on the entire 4KiB buffer before writing.

### M3: Buffer Pool (2-3 weeks)

The "cache" layer that prevents constant disk I/O.

**Goal**: An in-memory pool of pages with an eviction policy.
**Artifacts**: Buffer pool design doc; Benchmark: Cache hit rate vs. latency.

- Task 3.1: Create a `Frame` struct `page_data: [u8; 4096]`, `pin_count: u32`, `is_dirty: bool`.
- Task 3.2: Implement `LRUReplacer` using `HashMap` and a doubly-linked list (`std::collections::VecDeque`)
- Task 3.3: Build the `BufferPoolManager`:
  - `fetch_page(page_id)`: Checks cache, if miss, evicts a page (if `pin_count == 0`), reads from disk.
  - `unpin_page(page_id, is_dirty)`: Decrements pin count; if dirty, mark for eventual write.
  - `flush_page(page_id)`: Forces a write to `DiskManager`.

## Phase 2: Durability & Recovery

### M4: Minimal WAL (2-3 weeks)

The "Write-Ahead" rule: never write a page to disk before the log is safe.

**Goal**: Append-only log for redo operations.
**Artifacts**: WAL format specification; ADR "Redo-only vs Undo/Redo".

- Task 4.1: Define `LogRecord types`: `INSERT`, `DELETE`, `UPDATE`.
- Task 4.2: Implement `LogManager``: a background thread or a buffered writer that appends records to a`.log` file.
- Task 4.3: Implement Log Sequence Numbers (LSN): every log record gets a unique, increasing ID.
- Task 4.4: Enforce the Write-Ahead Rule: `BufferPool` must call `log_manager.flush()` before it allows `disk_manager.write_page()`.

### M5: Crash Recovery v1 (2-3 weeks)

The "Truth" is in the log, not the data file.

**Goal**: A startup routine that replays the WAL to the data file.
**Artifacts**: Recovery flow diagram.

- Task 5.1: Implement `RevoveryManager`.
- Task 5.2; The Redo Pass: Scan WAL from start to finish, re-applying every logged operation to the pages.
- Task 5.3: Write a "Chaos" tes: a separate binary that starts an insert loop and is killed by `SIGKILL`. Thee next run
must verify all data is present.

### M6: Page Allocation & Free Space (2 weeks)

Managing lifecycle of the file itself.

**Goal**: Track which pages are used and which are free for reuse.
**Artifacts**: ADR "Free List vs. Bitmap".

- Task 6.1: Implement `HeaderPage` (Page 0) to store DB metadata.
- Task 6.2: Build `FreeList`: a linked list of Page IDs that were deleted and are ready for reuse.
- Task 6.3: update `BufferPoolManager::new_page` to check the Free List before extending the file size.

## Phase 3: The Access Method

### M7: B+Tree Logic (In-Memory) (3-4 weeks)

Separating algorithmic complexity from disk complexity.

**Goal**: A pure Rust implementation of B+Tree search, insert, and split.
**Artifacts**: B+Tree invariants document: Logic diagrams.

- Task 7.1: Implement `Node` enum: `Internal(Vec<Key, PageId>)` or `Leaf(Vec<Key, Value>)`.
- Task 7.2: Implement binary search within a node.
- Task 7.3: Implement `split_node`: when a node exceeds `MAX_FANOUT`, move half the keys to a sibling and push the median key up.
- Task 7.4: Implement `merge_node` (coalescing) for deletions.

### M8: B+Tree on Pages (4-6 weeks)

The "Integration" milestone—this is often the hardest part of the project.

**Goal**: Map the B+Tree nodes onto the `SlottedPages` managed by the `BufferPool`.
**Artifacts**: ADR: "Leaf Format".

- Task 8.1: Replace memory pointers with `PageId`.
- Task 8.2: Create `LeafPage``and`InternalPage` wrappers that interpret the raw bytes from M2's `SlottedPage`.
- Task 8.3: Implement `BTree::get(key)`: traverses from the root page down to the leaf.
- Task 8.4: Implement `BTree::put(key, value)`: handles page splits and updating the parent's `PageId` pointers.

### M9: Range Scan / Iterator (2-3 weeks)

Moving beyond single-key lookups.

**Goal**: Implement `Next()` functionality.
**Artifacts**: Benchmark: Sequential vs. Random scan performance.

- Task 9.1: Add `next_page_id` to the `LeafPage` header.
- Task 9.2: Implement `BTreeIterator`: holds a pin on the current leaf page and an index within that page.
- Task 9.3: Implement the Rust `Iterator` trait so you can use for `(k, v) in db.range(start..end)`.

## Phase 4: hardening & Concurrency

### M10: Checkpoint (3-4 weeks)

Preventing the WAL from growing infinitely.

**Goal**: Periodically flush all dirty pages and truncate the log.
**Artifacts**: ADR "Fuzzy vs. Sharp Checkpointing".

- Task 10.1: Implement a `Checkpoint` record in the WAL.
- Task 10.2: Create the flush mechanism: 1. Stop new transactions, 2. Flush all dirty pages, 3. Write Checkpoint record, 4. Truncate WAL.
- Task 10.3: Optimize Recovery: skip WAL records prior to the last successful checkpoint.

### M11: Concurrency v1 (2-4 weeks)

Transition from single-threaded engine to a thread-safe one

**Goal**: Support multiple reader and one writter
**Artifacts**: ADR "Concurrency model: Latching Strategy"

- Task 11.1: Wrap `Page`` in`parking_lot::RwLock`.
- Task 11.2: Implement Latch Crabbing: To find a leaf, R-latch parent, R-latch child, then release parent latch.
- Task 11.3: For writes, use exclusive latches and hold them until the split/insert is complete.

### M12 - Hardening (Rest of Year)

Turning "project" into "product".

**Goal**: Robustness and documentation
**Artifacts**: "What I'd build next" post-mortem.

- Task 12.1: Integration testing with `quickcheck` or `arbitrary` to generate random KV sequences.
- Task 12.2: Write the "Technical Specification" PDF for your portfolio.
- Task 12.3: Profile with `flamegraph` to find bottlenecks in your `SlottedPage` compaction or Buffer Pool eviction.
