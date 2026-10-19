//! Positions of reopened anonymous files. A transferred host descriptor
//! shares its host offset with the sender; positional I/O and a separate
//! guest cursor preserve Linux's independent open-file-description offset.
//! The cursor is shared by guest dup/fork. A private POSIX record lock
//! serializes forked processes and is released by the kernel on owner exit.
//! The local mutex serializes threads; guest fork runs outside file I/O.

use std::fs::File;
use std::os::fd::AsRawFd;
use std::os::unix::fs::FileExt;
use std::os::unix::fs::MetadataExt;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use super::fd::{FileObject, FileType, OpenFile};
use super::memfd::{Memfd, seal};
use crate::user::linux::abi::errno::Errno;
use crate::user::linux::abi::errno_table::{EACCES, EINTR, EINVAL, EIO, ENOENT, EOPNOTSUPP};
use crate::user::linux::abi::open::{O_ACCMODE, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY};
use crate::user::linux::process::ProcState;
use crate::user::mm::{SharedWords, anonymous_file};

#[derive(Debug)]
pub(crate) struct Cursor {
    position: SharedWords,
    lock_file: File,
    local: Mutex<()>,
}

fn record_lock(file: &File, kind: libc::c_short) -> Result<(), Errno> {
    // SAFETY: flock consists of integer fields; zero initializes every field
    // and padding before fcntl reads the live, aligned structure. The owned
    // file descriptor remains open throughout this synchronous call.
    let mut lock: libc::flock = unsafe { std::mem::zeroed() };
    lock.l_type = kind;
    lock.l_whence = libc::SEEK_SET as libc::c_short;
    lock.l_len = 1;
    loop {
        // SAFETY: fcntl receives a valid descriptor, command, and pointer to
        // the initialized flock above. It neither retains nor aliases it.
        if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLKW, &lock) } == 0 {
            return Ok(());
        }
        let error = Errno::from(std::io::Error::last_os_error());
        if error.0 != EINTR {
            return Err(error);
        }
    }
}

struct Unlock<'a>(&'a File);

impl Drop for Unlock<'_> {
    fn drop(&mut self) {
        let _ = record_lock(self.0, libc::F_UNLCK as libc::c_short);
    }
}

impl Cursor {
    pub(crate) fn new() -> Result<Self, Errno> {
        Ok(Self {
            position: SharedWords::new(1)?,
            lock_file: anonymous_file()?,
            local: Mutex::new(()),
        })
    }

    fn with_position<T>(
        &self,
        operation: impl FnOnce(u64) -> Result<(u64, T), Errno>,
    ) -> Result<T, Errno> {
        let _local = self.local.lock().map_err(|_| Errno(EIO))?;
        record_lock(&self.lock_file, libc::F_WRLCK as libc::c_short)?;
        let _unlock = Unlock(&self.lock_file);
        let (next, value) = operation(self.position.words()[0].load(Ordering::Acquire))?;
        self.position.words()[0].store(next, Ordering::Release);
        Ok(value)
    }

    pub(crate) fn read(&self, file: &File, bytes: &mut [u8]) -> Result<usize, Errno> {
        self.with_position(|pos| {
            let count = file.read_at(bytes, pos)?;
            Ok((pos + count as u64, count))
        })
    }

    pub(crate) fn write(
        &self,
        file: &File,
        bytes: &[u8],
        append: bool,
        allowed: impl FnOnce(u64, usize) -> Result<usize, Errno>,
    ) -> Result<usize, Errno> {
        self.with_position(|pos| {
            let pos = if append { file.metadata()?.len() } else { pos };
            let count = allowed(pos, bytes.len())?;
            let written = file.write_at(&bytes[..count], pos)?;
            Ok((pos + written as u64, written))
        })
    }

    pub(crate) fn seek(&self, file: &File, offset: i64, whence: u32) -> Result<u64, Errno> {
        self.with_position(|pos| {
            let base = match whence {
                0 => 0,
                1 => pos,
                2 => file.metadata()?.len(),
                _ => return Err(Errno(EINVAL)),
            };
            let next = i64::try_from(base)
                .ok()
                .and_then(|base| base.checked_add(offset))
                .filter(|pos| *pos >= 0)
                .ok_or(Errno(EINVAL))? as u64;
            Ok((next, next))
        })
    }
}

/// Reopen only anonymous regular files whose seal set cannot change. Other
/// inode kinds and mutable remote seal state require additional transport.
pub(crate) fn export(
    p: &ProcState,
    fd: i32,
    flags: u32,
    (uid, gid, capable): (u32, u32, bool),
) -> Result<(File, Vec<u8>), Errno> {
    let same = uid == p.creds.0 && uid == p.creds.1 && gid == p.creds.2 && gid == p.creds.3;
    if !capable && (!same || p.dumpable != 1) {
        return Err(Errno(EACCES));
    }
    let source = p.fds.file(fd).map_err(|_| Errno(ENOENT))?;
    let FileObject::Host(file) = &source.object else {
        return Err(Errno(EOPNOTSUPP));
    };
    let Some(memfd) = &source.memfd else {
        return Err(Errno(EOPNOTSUPP));
    };
    if memfd.seals() & seal::SEAL == 0 || source.ftype != FileType::Regular {
        return Err(Errno(EOPNOTSUPP));
    }
    let metadata = file.metadata()?;
    let permission = if uid == metadata.uid() {
        (metadata.mode() >> 6) & 7
    } else if gid == metadata.gid() {
        (metadata.mode() >> 3) & 7
    } else {
        metadata.mode() & 7
    };
    let mut need = match flags & O_ACCMODE {
        O_RDONLY => 4,
        O_WRONLY => 2,
        O_RDWR => 6,
        _ => return Err(Errno(EINVAL)),
    };
    if flags & O_TRUNC != 0 {
        need |= 2;
    }
    if !capable && permission & need != need {
        return Err(Errno(EACCES));
    }
    if flags & O_TRUNC != 0 {
        memfd.check_resize(metadata.len(), 0)?;
        file.set_len(0)?;
    }
    let mut payload = memfd.seals().to_le_bytes().to_vec();
    payload.push(u8::from(memfd.hugetlb));
    payload.extend_from_slice(source.path.as_bytes());
    Ok((file.try_clone()?, payload))
}

pub(crate) fn adopt(file: File, metadata: &[u8], flags: u32) -> Result<Arc<OpenFile>, Errno> {
    let seal_bytes: [u8; 4] = metadata
        .get(..4)
        .ok_or(Errno(EIO))?
        .try_into()
        .map_err(|_| Errno(EIO))?;
    let seals = u32::from_le_bytes(seal_bytes);
    if seals & seal::SEAL == 0 {
        return Err(Errno(EIO));
    }
    let hugetlb = *metadata.get(4).ok_or(Errno(EIO))? != 0;
    let path =
        String::from_utf8(metadata.get(5..).ok_or(Errno(EIO))?.to_vec()).map_err(|_| Errno(EIO))?;
    let identity = super::identity(&file)?;
    let memfd = Memfd::new(seals, hugetlb, identity)?;
    let mut opened = OpenFile::with_memfd(
        FileObject::Host(file),
        FileType::Regular,
        path,
        None,
        flags,
        Some(memfd),
    );
    let cursor = Cursor::new()?;
    Arc::get_mut(&mut opened).ok_or(Errno(EIO))?.reopen_cursor = Some(cursor);
    Ok(opened)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user::linux::abi::Sysno;
    use crate::user::linux::tests::harness::{Harness, each_abi};
    use std::io::{Seek, Write};

    #[test]
    fn reopened_memfd_positions_and_immutable_seals_are_independent() {
        each_abi(|abi| {
            let mut h = Harness::new(abi);
            let name = h.scratch;
            h.proc.state.space.write(name, b"reopen\0").unwrap();
            let fd = h.ok(Sysno::MemfdCreate, &[name, 0]) as i32;
            let original = h.proc.state.fds.file(fd).unwrap();
            original.write(b"abcdef").unwrap();
            let creds = (h.proc.state.creds.1, h.proc.state.creds.3, false);
            let (file, payload) = export(&h.proc.state, fd, O_RDWR, creds).unwrap();
            let reopened = adopt(file, &payload, O_RDWR).unwrap();
            let mut bytes = [0; 2];
            assert_eq!(reopened.read(&mut bytes).unwrap(), 2);
            assert_eq!(&bytes, b"ab");
            assert_eq!(original.seek(0, 1).unwrap(), 6);
            assert_eq!(reopened.seek(-1, 1).unwrap(), 1);
            assert_eq!(reopened.write(b"Z").unwrap(), 1);
            original.read_at(&mut bytes, 0).unwrap();
            assert_eq!(&bytes, b"aZ");
            assert_eq!(original.seek(0, 1).unwrap(), 6);
            let mutable = h.ok(Sysno::MemfdCreate, &[name, 2]) as i32;
            assert!(matches!(
                export(&h.proc.state, mutable, O_RDWR, creds),
                Err(Errno(EOPNOTSUPP))
            ));
        });
    }

    #[test]
    fn reopened_cursor_serializes_readers_and_rejects_negative_seeks() {
        let mut file = anonymous_file().unwrap();
        file.write_all(&(0_u8..64).collect::<Vec<_>>()).unwrap();
        let file = Arc::new(file);
        let cursor = Arc::new(Cursor::new().unwrap());
        let threads: Vec<_> = (0..64)
            .map(|_| {
                let file = file.clone();
                let cursor = cursor.clone();
                std::thread::spawn(move || {
                    let mut byte = [0];
                    assert_eq!(cursor.read(&file, &mut byte).unwrap(), 1);
                    byte[0]
                })
            })
            .collect();
        let mut actual = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>();
        actual.sort_unstable();
        assert_eq!(actual, (0_u8..64).collect::<Vec<_>>());
        assert_eq!(cursor.seek(&file, 0, 1).unwrap(), 64);
        assert!(matches!(cursor.seek(&file, -65, 1), Err(Errno(EINVAL))));
        assert_eq!(cursor.seek(&file, 0, 1).unwrap(), 64);
        assert_eq!((&*file).stream_position().unwrap(), 64);
    }
}
