use super::{
    BlockDevice,
    DiskInode,
    DiskInodeType,
    DirEntry,
    EasyFileSystem,
    DIRENT_SZ,
    get_block_cache,
    block_cache_sync_all,
};
use alloc::sync::Arc;
use alloc::string::String;
use alloc::vec::Vec;
use spin::{Mutex, MutexGuard};

/// Virtual filesystem layer over easy-fs
pub struct Inode {
    inode_id: usize,
    block_id: usize,
    block_offset: usize,
    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
}

impl Inode {
    /// Create a vfs inode
    pub fn new(
        inode_id: u32,
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            inode_id: inode_id as usize,
            block_id: block_id as usize,
            block_offset,
            fs,
            block_device,
        }
    }
    /// Call a function over a disk inode to read it
    fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(
            self.block_id,
            Arc::clone(&self.block_device)
        ).lock().read(self.block_offset, f)
    }
    /// Call a function over a disk inode to modify it
    fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(
            self.block_id,
            Arc::clone(&self.block_device)
        ).lock().modify(self.block_offset, f)
    }
    /// Find inode under a disk inode by name
    fn find_inode_id(
        &self,
        name: &str,
        disk_inode: &DiskInode,
    ) -> Option<u32> {
        // assert it is a directory
        assert!(disk_inode.is_dir());
        let file_count = (disk_inode.size as usize) / DIRENT_SZ;
        let mut dirent = DirEntry::empty();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(
                    DIRENT_SZ * i,
                    dirent.as_bytes_mut(),
                    &self.block_device,
                ),
                DIRENT_SZ,
            );
            if dirent.name() == name {
                return Some(dirent.inode_number() as u32);
            }
        }
        None
    }
    /// Find inode under current inode by name
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode)
            .map(|inode_id| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                Arc::new(Self::new(
                    inode_id,
                    block_id,
                    block_offset,
                    self.fs.clone(),
                    self.block_device.clone(),
                ))
            })
        })
    }
    /// Increase the size of a disk inode
    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size < disk_inode.size {
            return;
        }
        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut v: Vec<u32> = Vec::new();
        for _ in 0..blocks_needed {
            v.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, v, &self.block_device);
    }
    /// Create inode under current inode by name
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        let mut fs = self.fs.lock();
        if self.modify_disk_inode(|root_inode| {
            // assert it is a directory
            assert!(root_inode.is_dir());
            // has the file been created?
            self.find_inode_id(name, root_inode)
        }).is_some() {
            return None;
        }
        // create a new file
        // alloc a inode with an indirect block
        let new_inode_id = fs.alloc_inode();
        // initialize inode
        let (new_inode_block_id, new_inode_block_offset) 
            = fs.get_disk_inode_pos(new_inode_id);
        get_block_cache(
            new_inode_block_id as usize,
            Arc::clone(&self.block_device)
        ).lock().modify(new_inode_block_offset, |new_inode: &mut DiskInode| {
            new_inode.initialize(DiskInodeType::File);
        });
        self.modify_disk_inode(|root_inode| {
            // append file in the dirent
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            // increase size
            self.increase_size(new_size as u32, root_inode, &mut fs);
            // write dirent
            let dirent = DirEntry::new(name, new_inode_id);
            root_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });

        let (block_id, block_offset) = fs.get_disk_inode_pos(new_inode_id);
        block_cache_sync_all();
        // return inode
        Some(Arc::new(Self::new(
            new_inode_id,
            block_id,
            block_offset,
            self.fs.clone(),
            self.block_device.clone(),
        )))
        // release efs lock automatically by compiler
    }
    /// List inodes under current inode
    pub fn ls(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut v: Vec<String> = Vec::new();
            for i in 0..file_count {
                let mut dirent = DirEntry::empty();
                assert_eq!(
                    disk_inode.read_at(
                        i * DIRENT_SZ,
                        dirent.as_bytes_mut(),
                        &self.block_device,
                    ),
                    DIRENT_SZ,
                );
                v.push(String::from(dirent.name()));
            }
            v
        })
    }
    /// Read data from current inode
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            disk_inode.read_at(offset, buf, &self.block_device)
        })
    }
    /// Write data to current inode
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all();
        size
    }
    /// Clear the data in current inode
    pub fn clear(&self) {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            let size = disk_inode.size;
            let data_blocks_dealloc = disk_inode.clear_size(&self.block_device);
            assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
            for data_block in data_blocks_dealloc.into_iter() {
                fs.dealloc_data(data_block);
            }
        });
        block_cache_sync_all();
    }    
    
    
    pub fn link(&self, old_name: &str, new_name: &str) -> isize {
        if old_name == new_name {
            return -1;
        }
        if let Some(old_inode) = self.find(old_name) {
            let mut fs = self.fs.lock();
            self.modify_disk_inode(|root_inode| {
                let file_count = (root_inode.size as usize) / DIRENT_SZ;
                let new_size = (file_count + 1) * DIRENT_SZ;
                self.increase_size(new_size as u32, root_inode, &mut fs);
                let dirent = DirEntry::new(new_name, old_inode.inode_id as u32);
                root_inode.write_at(
                    file_count * DIRENT_SZ,
                    dirent.as_bytes(),
                    &self.block_device,
                );
            });
            old_inode.modify_disk_inode(|disk_inode: &mut DiskInode| {
                disk_inode.nlink += 1;
                disk_inode.nlink
            });
            block_cache_sync_all();
            return 0;
        }
        -1
    }

    // pub fn unlink(&self, name: &str) -> isize {
    //     if let Some(inode) = self.find(name) {
    //         let nlink = inode.modify_disk_inode(|disk_inode: &mut DiskInode| {
    //             disk_inode.nlink -= 1;
    //             disk_inode.nlink
    //         });

    //         let file_count = self.read_disk_inode(|root_inode| {
    //              (root_inode.size as usize) / DIRENT_SZ
    //         });
    //         let mut dirent = DirEntry::new(&name, inode.inode_id as u32);
    //         for i in 0..file_count {
    //             assert_eq!(
    //                 self.read_at(DIRENT_SZ * i, dirent.as_bytes_mut()),
    //                 DIRENT_SZ,
    //             );
    //             if dirent.name() == name {
    //                 let new_dirent = DirEntry::empty();
    //                 self.write_at(i * DIRENT_SZ, new_dirent.as_bytes());
    //             }
    //         }

    //         if nlink == 0 {
    //             inode.clear();
    //         }
    //         block_cache_sync_all();
    //         return 0;
    //     }
    //     -1
    // }


    pub fn unlink(&self, name: &str) -> isize {
        let mut fs = self.fs.lock();
        let mut to_unlink_inode_id: Option<u32> = None;
        let mut new_dirent_vec: Vec<DirEntry> = Vec::new();
        self.read_disk_inode(|root_inode| {
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            for i in 0..file_count {
                let mut dirent = DirEntry::empty();
                assert_eq!(
                    root_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device,),
                    DIRENT_SZ,
                );
                if dirent.name() != name {
                    new_dirent_vec.push(dirent);
                } else {
                    to_unlink_inode_id = Some(dirent.inode_number());
                }
            }
        });
        self.modify_disk_inode(|root_inode| {
            let size = root_inode.size;
            let data_blocks_dealloc = root_inode.clear_size(&self.block_device);
            assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
            for data_block in data_blocks_dealloc.into_iter() {
                fs.dealloc_data(data_block);
            }
            self.increase_size((new_dirent_vec.len() * DIRENT_SZ) as u32, root_inode, &mut fs);
            for (i, dirent) in new_dirent_vec.iter().enumerate() {
                root_inode.write_at(i * DIRENT_SZ, dirent.as_bytes(), &self.block_device);
            }
        });
        if to_unlink_inode_id.is_none() {
            return -1;
        }
        let (block_id, block_offset) = fs.get_disk_inode_pos(to_unlink_inode_id.unwrap());
        get_block_cache(block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(block_offset, |di: &mut DiskInode| {
                di.nlink -= 1;
                if di.nlink == 0 {
                    let size = di.size;
                    let data_blocks_dealloc = di.clear_size(&self.block_device);
                    assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
                    for data_block in data_blocks_dealloc.into_iter() {
                        fs.dealloc_data(data_block);
                    }
                }
             });
        block_cache_sync_all();
        0
    }


    pub fn nlink(&self) -> u32 {
        self.read_disk_inode(|disk_inode|{
            disk_inode.nlink
        })
    }

    pub fn inode_id(&self) -> u64 {
        self.inode_id as u64
    }
    pub fn is_dir(&self) -> bool {
        self.read_disk_inode(|disk_inode|{
            if disk_inode.is_dir() == true {
                true
            } else {
                false
            }
        })
    }
}
