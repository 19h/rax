use super::{Link, Msg};
use std::os::fd::AsRawFd;

// Descriptor lifetime checks require the last reference to close. Other tests
// fork concurrently and inherit even CLOEXEC descriptors until exec or exit.
// Execute these checks in a fresh process with exactly one test thread.
fn isolated(name: &str) -> bool {
    let name = format!("user::linux::ptrace::adoption_tests::{name}");
    const KEY: &str = "RAX_PTRACE_ADOPTION_TEST";
    if std::env::var(KEY).as_deref() == Ok(name.as_str()) {
        return false;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &name, "--test-threads=1", "--nocapture"])
        .env(KEY, &name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{name}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
    true
}

fn offered() -> (Link, Link, Link, Link, i32) {
    let (ancestor, parent) = Link::pair().unwrap();
    let (offered, mut child) = Link::pair().unwrap();
    let retained = Link::from_fd(offered.fd.try_clone().unwrap());
    let watch = parent.fd.try_clone().unwrap();
    let watch_fd = watch.as_raw_fd();
    child.retain_adoption(retained, watch);
    (ancestor, parent, offered, child, watch_fd)
}

#[test]
fn acknowledgement_releases_both_retained_descriptors_and_preserves_frames() {
    if isolated("acknowledgement_releases_both_retained_descriptors_and_preserves_frames") {
        return;
    }
    let (mut ancestor, parent, offered, mut child, watch_fd) = offered();
    assert!(parent.send_passing(&Msg::Relative { pid: 7 }, &offered));
    drop(offered);
    drop(parent);
    assert_eq!(child.adoption_watch_fd(), Some(watch_fd));
    assert_eq!(ancestor.recv(), [Msg::Relative { pid: 7 }]);
    let adopted = Link::from_fd(ancestor.take_fd().unwrap());
    assert!(adopted.adoption_ready(true));
    let reply = Msg::Reply {
        ret: 12,
        payload: b"after acknowledgement".to_vec(),
    };
    assert!(adopted.send(&reply));
    assert_eq!(child.recv(), [reply]);
    assert!(!child.closed);
    assert_eq!(child.adoption_watch_fd(), None);
    ancestor.recv();
    assert!(
        ancestor.closed,
        "the intermediate route can close after acknowledgement"
    );
    drop(adopted);
    child.recv();
    assert!(
        child.closed,
        "the offered endpoint must also be released after acknowledgement"
    );
}

#[test]
fn ancestor_exit_before_receiving_the_offer_releases_the_retained_endpoint() {
    if isolated("ancestor_exit_before_receiving_the_offer_releases_the_retained_endpoint") {
        return;
    }
    let (ancestor, parent, offered, mut child, watch_fd) = offered();
    assert!(parent.send_passing(&Msg::Relative { pid: 7 }, &offered));
    drop(offered);
    drop(ancestor);
    let ready = super::super::host::poll(&[(watch_fd, false, false)], 1000).unwrap();
    assert!(ready[0].hangup || ready[0].error);
    assert!(child.recv().is_empty());
    assert!(child.closed);
    assert_eq!(child.adoption_watch_fd(), None);
}

#[test]
fn rejected_publication_closes_the_unadopted_child_without_waiting_for_ancestor_exit() {
    let (_ancestor, _parent, offered, mut child, _) = offered();
    assert!(offered.adoption_ready(false));
    drop(offered);
    assert!(child.recv().is_empty());
    assert!(child.closed);
    assert_eq!(child.adoption_watch_fd(), None);
}

#[test]
fn acknowledged_socket_transfers_survive_immediate_original_and_parent_release() {
    for index in 0..50_000 {
        let (mut ancestor, parent, offered, mut child, _) = offered();
        assert!(parent.send_passing(&Msg::Relative { pid: 7 }, &offered));
        drop(offered);
        drop(parent);
        assert_eq!(
            ancestor.recv(),
            [Msg::Relative { pid: 7 }],
            "transfer {index}"
        );
        let mut adopted = Link::from_fd(ancestor.take_fd().unwrap());
        assert!(adopted.adoption_ready(true));
        assert!(child.recv().is_empty());
        assert!(!child.closed, "child transfer {index}");
        assert_eq!(child.adoption_watch_fd(), None);
        assert!(adopted.recv().is_empty());
        assert!(!adopted.closed, "ancestor transfer {index}");
        let message = Msg::Reply {
            ret: index,
            payload: Vec::new(),
        };
        assert!(child.send(&message));
        assert_eq!(adopted.recv(), [message], "data transfer {index}");
    }
}
