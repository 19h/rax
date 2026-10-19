//! Live read-only procfs queries over tracing links. The target reads its
//! own guest state at a scheduler boundary, including while ptrace-stopped.
//! Replies are addressed to the requesting guest thread so concurrent file
//! opens cannot consume each other's or ptrace's replies.

use super::{LinkId, Msg, link_mut, link_to, send};
use crate::user::linux::abi::errno::Errno;
use crate::user::linux::abi::errno_table::{EIO, ENOENT, ESRCH};
use crate::user::linux::process::{ProcState, Threads};
use crate::user::linux::procfs::{self, ProcEntry};
use crate::user::linux::syscall::Ctx;
use crate::user::linux::wait::{Resume, Wait};

/// Captured pathname and open parameters survive an asynchronous query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenRequest {
    pub tid: i32,
    pub name: String,
    pub guest: String,
    pub flags: u32,
    pub create_mode: u32,
}

pub(crate) fn read(
    c: &mut Ctx<'_>,
    request: OpenRequest,
) -> Result<(Vec<u8>, Option<std::fs::File>), Errno> {
    let link = if let Some(Resume::TraceeFile { link, .. }) = c.resume.as_ref() {
        let link = *link;
        c.resume = None;
        let replies = &mut c.p.tracees.proc_replies;
        if let Some(index) = replies.iter().position(|r| r.0 == link && r.1 == c.t.tid) {
            let (_, _, ret, payload, file) = replies.remove(index);
            return if ret < 0 {
                Err(Errno(-ret as i32))
            } else {
                Ok((payload, file))
            };
        }
        link
    } else {
        let link =
            c.p.tracees
                .get(request.tid)
                .map(|t| t.link)
                .or_else(|| link_to(c.p, request.tid))
                .ok_or(Errno(ESRCH))?;
        if !send(
            c.p,
            link,
            &Msg::ReadProc {
                tid: request.tid,
                requester: c.t.tid,
                name: request.name.clone(),
                flags: request.flags,
                uid: c.p.creds.1,
                gid: c.p.creds.3,
                capable: c.p.creds.1 == 0,
            },
        ) {
            return Err(Errno(ESRCH));
        }
        link
    };
    let peer = link_mut(c.p, link)
        .filter(|link| !link.closed)
        .ok_or(Errno(ESRCH))?;
    let mut wait = Wait::fd(peer.fd(), true, false);
    // Finish the in-flight protocol rather than starting a second request
    // when a guest signal arrives.
    wait.interruptible = false;
    Err(c.block(wait, Resume::TraceeFile { link, request }))
}

pub(crate) fn serve(
    p: &ProcState,
    threads: &Threads<'_>,
    link: LinkId,
    tid: i32,
    name: &str,
    flags: u32,
    creds: (u32, u32, bool),
) -> (i64, Vec<u8>, Option<std::fs::File>) {
    #[cfg(unix)]
    if let Some(fd) = name.strip_prefix("fd/").and_then(|n| n.parse::<i32>().ok()) {
        if !threads.iter().any(|t| t.tid == tid) {
            return (-i64::from(ESRCH), Vec::new(), None);
        }
        return match super::super::fs::reopen::export(p, fd, flags, creds) {
            Ok((file, bytes)) => (0, bytes, Some(file)),
            Err(Errno(errno)) => (-i64::from(errno), Vec::new(), None),
        };
    }
    let result = (|| {
        let thread = threads.iter().find(|t| t.tid == tid).ok_or(Errno(ESRCH))?;
        if !thread
            .ptrace
            .as_ref()
            .is_some_and(|tr| tr.tracer >= 0 && tr.link == link)
        {
            return Err(Errno(ESRCH));
        }
        if !matches!(
            name,
            "maps" | "auxv" | "cmdline" | "environ" | "stat" | "status" | "comm"
        ) {
            return Err(Errno(ENOENT));
        }
        let all = threads.iter().collect::<Vec<_>>();
        let path = format!("/proc/{tid}/{name}");
        match procfs::lookup(p, thread, &all, &path) {
            Some(ProcEntry::File(bytes)) | Some(ProcEntry::Comm { text: bytes, .. }) => Ok(bytes),
            None => Err(Errno(ENOENT)),
            _ => Err(Errno(EIO)),
        }
    })();
    match result {
        Ok(bytes) => (0, bytes, None),
        Err(Errno(errno)) => (-i64::from(errno), Vec::new(), None),
    }
}
