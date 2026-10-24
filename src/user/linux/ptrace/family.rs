//! Direct ancestor/descendant tracing links. Each fork prepares a new pair
//! per live ancestor before creating the host child. Its parent passes one
//! end to that ancestor; the child retains the other. No intermediate process
//! must stay alive to relay ptrace requests, replies, stops, or exits.
//!
//! A fork at depth d uses O(d) descriptors, time, and storage. Existing
//! links are never shared by two readers after the child has initialized.

use super::{Link, LinkId, Msg, link_ref};
use crate::user::linux::abi::errno::Errno;
use crate::user::linux::abi::errno_table::EINVAL;
use crate::user::linux::process::ProcState;
use crate::user::linux::syscall::{Ctx, SysResult};

/// `yama_task_prctl(PR_SET_PTRACER)`: zero clears the exception, -1
/// permits any tracer, and another value must identify an existing task.
/// This personality has no additional Yama scope restriction; credential
/// and dumpability checks still run at every actual attach.
pub(crate) fn set_ptracer(c: &Ctx<'_>, value: u64) -> SysResult {
    let pid = value as i32;
    if value == 0 || pid == -1 || c.is_own_tid(pid) || super::link_to(c.p, pid).is_some() {
        return Ok(0);
    }
    #[cfg(unix)]
    if c.p.config.host_services && pid > 0 {
        use crate::user::linux::abi::errno_table::ESRCH;
        if !matches!(crate::user::linux::host::kill(pid, 0), Err(Errno(ESRCH))) {
            return Ok(0);
        }
    }
    Err(Errno(EINVAL))
}

struct AncestorLink {
    route: LinkId,
    pid: i32,
    offered: Link,
    child: Link,
    #[cfg(unix)]
    watch: std::os::fd::OwnedFd,
}

pub(crate) struct ForkLinks {
    ancestors: Vec<AncestorLink>,
}

impl ForkLinks {
    #[cfg(unix)]
    pub(crate) fn prepare(p: &ProcState) -> Result<Self, Errno> {
        let mut ancestors = Vec::new();
        let parent = p.parent_link.as_ref().map(|_| (LinkId::Parent, p.ppid));
        let earlier = p
            .ancestor_pids
            .iter()
            .map(|pid| (LinkId::Adopted(*pid), *pid));
        for (id, pid) in parent.into_iter().chain(earlier) {
            if let Some(source) = link_ref(p, id).filter(|link| !link.closed) {
                let (ancestor_end, child_end) = Link::pair()?;
                let watch = source.fd.try_clone().map_err(Errno::from)?;
                ancestors.push(AncestorLink {
                    route: id,
                    pid,
                    offered: ancestor_end,
                    child: child_end,
                    watch,
                });
            }
        }
        Ok(Self { ancestors })
    }

    #[cfg(unix)]
    pub(crate) fn publish(self, p: &ProcState, child: i32) {
        for entry in self.ancestors {
            drop(entry.child);
            let sent = link_ref(p, entry.route).is_some_and(|link| {
                link.send_passing(&Msg::Relative { pid: child }, &entry.offered)
            });
            if !sent {
                entry.offered.adoption_ready(false);
            }
        }
    }

    #[cfg(unix)]
    pub(crate) fn inherit(self, p: &mut ProcState) {
        p.ancestor_pids.clear();
        for entry in self.ancestors {
            let mut child = entry.child;
            child.retain_adoption(entry.offered, entry.watch);
            p.ancestor_pids.push(entry.pid);
            p.adopted.push((entry.pid, child));
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::user::linux::abi::Sysno;
    use crate::user::linux::process::Threads;
    use crate::user::linux::ptrace::{self, req, tracee};
    use crate::user::linux::tests::harness::{Harness, each_abi};

    fn identity(h: &mut Harness, pid: i32) {
        h.proc.state.pid = pid;
        h.proc.threads[0].tid = pid;
    }

    fn poll(h: &mut Harness) {
        let mut threads = Threads::split(&mut h.proc.threads, None);
        tracee::poll_links(&mut h.proc.state, &mut threads);
    }

    // Duplicate the descriptors as host fork does, then let each branch
    // release the opposite ends through the production methods.
    fn fork_copy(links: &ForkLinks) -> ForkLinks {
        ForkLinks {
            ancestors: links
                .ancestors
                .iter()
                .map(|entry| AncestorLink {
                    route: entry.route,
                    pid: entry.pid,
                    offered: Link::from_fd(entry.offered.fd.try_clone().unwrap()),
                    child: Link::from_fd(entry.child.fd.try_clone().unwrap()),
                    watch: entry.watch.try_clone().unwrap(),
                })
                .collect(),
        }
    }

    #[test]
    fn grandchild_ptrace_links_survive_the_intermediate_process() {
        each_abi(|abi| {
            let mut ancestor = Harness::new(abi);
            let mut parent = Harness::new(abi);
            let mut child = Harness::new(abi);
            let (a, b, c) = (500_001, 500_002, 500_003);
            identity(&mut ancestor, a);
            identity(&mut parent, b);
            identity(&mut child, c);
            ancestor.proc.state.exe_path = "/guest/ancestor-image".into();
            parent.proc.state.ppid = a;
            child.proc.state.ppid = b;
            let (ancestor_end, parent_end) = Link::pair().unwrap();
            ancestor.proc.state.adopted.push((b, ancestor_end));
            parent.proc.state.parent_link = Some(parent_end);
            let links = ForkLinks::prepare(&parent.proc.state).unwrap();
            let inherited = fork_copy(&links);
            links.publish(&parent.proc.state, c);
            inherited.inherit(&mut child.proc.state);
            for (_, link) in &child.proc.state.adopted {
                let watch = link.adoption_watch_fd().unwrap();
                assert!(ptrace::link_fds(&child.proc.state).contains(&(watch, false, false)));
            }
            poll(&mut ancestor);
            drop(parent);
            poll(&mut ancestor);

            assert_eq!(
                ptrace::link_to(&ancestor.proc.state, c),
                Some(LinkId::Adopted(c))
            );
            assert_eq!(
                ptrace::link_to(&child.proc.state, a),
                Some(LinkId::Adopted(a))
            );
            assert_eq!(
                child.start(0, Sysno::Ptrace, &[req::ATTACH, a as u64, 0, 0]),
                None
            );
            poll(&mut ancestor);
            ancestor.proc.deliver_signals(0);
            assert!(tracee::parked(&ancestor.proc.threads[0]));
            assert_eq!(child.proc.wake_sleepers(), 1);
            assert_eq!(child.result(0), 0);
            assert_eq!(
                child.proc.state.tracees.get(a).unwrap().stopped,
                Some(crate::user::linux::signal::SIGSTOP)
            );
            let current = &child.proc.threads[0];
            assert!(matches!(
                crate::user::linux::procfs::lookup(&child.proc.state, current, &[current], &format!("/proc/{a}/exe")),
                Some(crate::user::linux::procfs::ProcEntry::Link(path)) if path == "/guest/ancestor-image"
            ));

            let word = ancestor.scratch;
            ancestor.fill(word, 8, 0x5a);
            assert_eq!(
                child.start(
                    0,
                    Sysno::Ptrace,
                    &[req::PEEKDATA, a as u64, word, child.scratch]
                ),
                None
            );
            poll(&mut ancestor);
            assert_eq!(child.proc.wake_sleepers(), 1);
            assert_eq!(child.result(0), 0);
            let mut bytes = [0; 8];
            child
                .proc
                .state
                .space
                .read(child.scratch, &mut bytes)
                .unwrap();
            assert_eq!(bytes, [0x5a; 8]);
            // A fresh open reads current guest VMAs, including a change
            // after the ptrace stop. Retrying retains the captured path.
            let output = child.anon(4096, 3, false);
            let pathname = child.scratch + 0x100;
            let maps = format!("/proc/{a}/maps\0");
            child
                .proc
                .state
                .space
                .write(pathname, maps.as_bytes())
                .unwrap();
            assert_eq!(
                child.start(0, Sysno::Openat, &[(-100_i64) as u64, pathname, 0, 0]),
                None
            );
            child
                .proc
                .state
                .space
                .write(pathname, b"/changed/while/blocked\0")
                .unwrap();
            ancestor
                .proc
                .state
                .space
                .map(
                    0x1234_0000,
                    4096,
                    crate::user::mm::Mapping::anonymous(crate::user::mm::Perms::READ),
                )
                .unwrap();
            let expected = crate::user::linux::procfs::maps(&ancestor.proc.state);
            poll(&mut ancestor);
            assert_eq!(child.proc.wake_sleepers(), 1);
            let fd = child.result(0);
            assert!(fd >= 0, "remote maps open: {fd}");
            assert!(expected.len() < 4096);
            assert_eq!(
                child.ok(Sysno::Read, &[fd as u64, output, expected.len() as u64 + 1]),
                expected.len() as u64
            );
            let mut actual = vec![0; expected.len()];
            child.proc.state.space.read(output, &mut actual).unwrap();
            assert_eq!(actual, expected);
            for (_, link) in &child.proc.state.adopted {
                // Concurrent host-fork tests can briefly inherit descriptors.
                // Model loss of this peer independently of those extra copies.
                // SAFETY: Link owns this live socket; shutdown retains ownership.
                assert_eq!(unsafe { libc::shutdown(link.fd(), libc::SHUT_RDWR) }, 0);
            }
            drop(child);
            let link = ptrace::link_to(&ancestor.proc.state, c).unwrap();
            let fd = link_ref(&ancestor.proc.state, link).unwrap().fd();
            let ready = crate::user::linux::host::poll(&[(fd, false, false)], 1000).unwrap();
            assert!(ready[0].hangup || ready[0].error);
            poll(&mut ancestor);
            assert_eq!(tracee::tracer_pid(&ancestor.proc.threads[0]), 0);
            assert!(!tracee::parked(&ancestor.proc.threads[0]));
            assert!(ancestor.proc.state.adopted.is_empty());
        });
    }

    #[test]
    fn set_ptracer_validates_tasks_and_special_values() {
        each_abi(|abi| {
            let mut h = Harness::new(abi);
            let me = h.proc.state.pid as u64;
            for pid in [0, me, u64::MAX, u64::from(u32::MAX)] {
                assert_eq!(h.call(Sysno::Prctl, &[0x5961_6d61, pid, 1, 2, 3]), 0);
            }
            assert_eq!(
                h.call(Sysno::Prctl, &[0x5961_6d61, 0x7fff_ffff]),
                -i64::from(EINVAL)
            );
        });
    }
}
