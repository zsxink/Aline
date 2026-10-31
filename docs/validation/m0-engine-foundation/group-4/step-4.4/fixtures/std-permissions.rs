// 独立探针：仅在外层创建的私有临时目录操作，用于暴露 std Permissions 覆盖范围。
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let directory = std::env::args_os().nth(1).ok_or("缺少探针目录")?;
    let directory = PathBuf::from(directory);
    let target = directory.join("target.md");
    let temporary = directory.join("temporary.md");
    let permissions = fs::metadata(&target)?.permissions();
    let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary)?;
    file.write_all(b"NEW\n")?;
    file.set_permissions(permissions)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, target)?;
    Ok(())
}
