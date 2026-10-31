// 独立探针：仅在外层创建的私有临时目录操作，模拟临时路径在复制前被替换。
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::os::unix::fs::symlink;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let directory = std::env::args_os().nth(1).ok_or("缺少探针目录")?;
    let directory = PathBuf::from(directory);
    let target = directory.join("target.md");
    let temporary = directory.join("temporary.md");
    let file = OpenOptions::new().write(true).create_new(true).open(&temporary)?;
    drop(file);
    fs::remove_file(&temporary)?;
    symlink(&target, &temporary)?;
    let result = fs::copy(&target, &temporary);
    println!("copy_result={result:?}");
    println!("target_bytes={:?}", fs::read(&target)?);
    Ok(())
}
