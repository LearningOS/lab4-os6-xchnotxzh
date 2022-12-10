# 功能总结

扩展 easy-fs 和内核实现下列功能。 

## linkat

- 实现功能 -- 给根节点下的已有名字的文件增加硬链接，也就是别名
- 实现策略 -- 在 `DiskInode` 中添加硬链接计数，增加硬链接意味着
  - 要找到旧名字文件的inode_id
  - 修改根节点元数据 -- 根节点`DiskInode`要硬链接计数递增
  - 修改根节点数据 -- 根节点`DiskInode`索引的数据，增加一个DirEntry(newname, inode_id)。

## unlinkat

- 实现功能 -- 删除硬链接，也就是取消别名
- 实现策略 -- 给同一个inode删除一个DirEntry，若变成0个，则先删inode？
  - 要找名字文件的inode_id
  - 修改根节点元数据 -- 根节点`DiskInode`要硬链接计数递减
  - 修改根节点数据 -- 根节点`DiskInode`索引的数据，去掉一个DirEntry(name, inode_id)。

## fstat

- 实现功能 -- 获取文件状态
- 实现策略
  - 通过fd获取对应的OSInode
  - 然后通过 `OSInode` 获取 `Inode`和 `DiskInode` 的相关信息填充到 `Stat` 中去。

# 问答题

## 1

### 问题

在我们的easy-fs中，root inode起着什么作用？如果root inode中的内容损坏了，会发生什么？

### 解答

整个文件系统的文件无法访问 -- 根目录是所有文件的总入口，它损坏意味着无法访问文件，包括应用程序。

# 建议

无