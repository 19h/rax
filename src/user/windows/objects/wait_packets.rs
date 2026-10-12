//! Association ownership and completion payloads; no native handles or threads.
use super::{ObjId, Object, Objects};
use crate::user::windows::nt::status::{STATUS_CANCELLED, STATUS_PENDING};
use std::collections::{BTreeMap, HashMap};

/// Pointer-width data carried by an NT I/O completion packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompletionPacket {
    /// Caller key context, truncated only by the guest ABI.
    pub key: u64,
    /// Caller APC/overlapped context, truncated only by the guest ABI.
    pub apc: u64,
    /// NTSTATUS, including failure statuses.
    pub status: u32,
    /// ULONG_PTR information, not restricted to a Win32 DWORD.
    pub information: u64,
    wait_packet: Option<ObjId>,
}
impl CompletionPacket {
    /// Constructs a completion independent of a wait-packet association.
    pub fn new(key: u64, apc: u64, status: u32, information: u64) -> Self {
        Self {
            key,
            apc,
            status,
            information,
            wait_packet: None,
        }
    }
}
#[derive(Debug)]
struct Association {
    port: ObjId,
    target: Option<ObjId>,
    ordinal: u64,
    completion: CompletionPacket,
}
#[derive(Debug, Default)]
pub(super) struct PacketAssociations {
    entries: HashMap<ObjId, Association>,
    pending: BTreeMap<u64, ObjId>,
    next: u64,
}
impl Objects {
    pub(crate) fn wait_packet_busy(&self, packet: ObjId) -> bool {
        self.packets.entries.contains_key(&packet)
    }
    pub(crate) fn associate_wait_packet(
        &mut self,
        packet: ObjId,
        port: ObjId,
        target: ObjId,
        mut completion: CompletionPacket,
    ) -> Result<(), &'static str> {
        if !matches!(self.obj(packet), Some(Object::WaitCompletionPacket { .. }))
            || !matches!(self.obj(port), Some(Object::CompletionPort { .. }))
            || self.obj(target).is_none()
            || self.wait_packet_busy(packet)
        {
            return Err("invalid wait-packet association invariant");
        }
        let ordinal = self
            .packets
            .next
            .checked_add(1)
            .ok_or("wait-packet order exhausted")?;
        self.retain_many(&[port, target])?;
        completion.wait_packet = Some(packet);
        self.packets.entries.insert(
            packet,
            Association {
                port,
                target: Some(target),
                ordinal,
                completion,
            },
        );
        self.packets.pending.insert(ordinal, packet);
        self.packets.next = ordinal;
        Ok(())
    }
    pub(crate) fn pending_wait_packets(&self) -> Vec<ObjId> {
        self.packets.pending.values().copied().collect()
    }
    pub(crate) fn wait_packet_target(&self, packet: ObjId) -> Option<ObjId> {
        self.packets.entries.get(&packet)?.target
    }
    // Capacity is reserved before consuming an automatic target signal.
    pub(crate) fn prepare_wait_packet(&mut self, packet: ObjId) -> Result<(), &'static str> {
        let association = self
            .packets
            .entries
            .get(&packet)
            .ok_or("missing wait-packet association")?;
        if association.target.is_none() {
            return Err("wait packet already queued");
        }
        let Some(Object::CompletionPort { queue, .. }) = self.obj_mut(association.port) else {
            return Err("wait-packet completion port disappeared");
        };
        queue
            .try_reserve(1)
            .map_err(|_| "wait-packet queue allocation failed")
    }
    pub(crate) fn complete_wait_packet(&mut self, packet: ObjId) {
        let association = self
            .packets
            .entries
            .get_mut(&packet)
            .expect("prepared association");
        let target = association.target.take().expect("pending target");
        let (port, completion, ordinal) = (
            association.port,
            association.completion,
            association.ordinal,
        );
        self.packets.pending.remove(&ordinal);
        let Some(Object::CompletionPort { queue, .. }) = self.obj_mut(port) else {
            unreachable!("pinned port")
        };
        queue.push_back(completion);
        // Asynchronous final release has no close caller. FileLifetime::Drop
        // retains deferred deletion diagnostics for a synchronous file target.
        self.release(target);
    }
    pub(crate) fn cancel_wait_packet(&mut self, packet: ObjId, remove: bool) -> u32 {
        let Some(association) = self.packets.entries.get(&packet) else {
            return STATUS_CANCELLED;
        };
        if association.target.is_none() && !remove {
            return STATUS_PENDING;
        }
        self.forget_wait_packet(packet);
        0
    }
    pub(super) fn forget_wait_packet(&mut self, packet: ObjId) {
        let Some(association) = self.packets.entries.remove(&packet) else {
            return;
        };
        self.packets.pending.remove(&association.ordinal);
        if association.target.is_none()
            && let Some(Object::CompletionPort { queue, .. }) = self.obj_mut(association.port)
        {
            queue.retain(|completion| completion.wait_packet != Some(packet));
        }
        self.release(association.port);
        if let Some(target) = association.target {
            self.release(target);
        }
    }
    // The closing handle retains the port until all queued association pins
    // have been released. Taking the queue leaves pending associations intact.
    pub(super) fn discard_completion_queue(&mut self, port: ObjId) {
        let Some(Object::CompletionPort { queue, .. }) = self.obj_mut(port) else {
            return;
        };
        let completions = std::mem::take(queue);
        for completion in completions {
            if let Some(packet) = completion.wait_packet {
                self.forget_wait_packet(packet);
            }
        }
    }
    /// Internal delivery primitive. The NT dequeue service separately owns
    /// thread concurrency, blocking, output probing and guest serialization.
    pub(crate) fn dequeue_completion(&mut self, port: ObjId) -> Option<CompletionPacket> {
        let Some(Object::CompletionPort { queue, .. }) = self.obj_mut(port) else {
            return None;
        };
        let completion = queue.pop_front()?;
        if let Some(packet) = completion.wait_packet {
            self.forget_wait_packet(packet);
        }
        Some(completion)
    }
}
