//! Poll associations in registration order before ordinary object waits.
use super::*;

/// O(P + B log P) expected time, O(P) temporary space for P pending packets
/// and B completions. Handle tables are never consulted after association.
pub(crate) fn pump_wait_packets(p: &mut Proc) -> Result<(), SyncError> {
    for packet in p.objects.pending_wait_packets() {
        let target = p
            .objects
            .wait_packet_target(packet)
            .ok_or(SyncError::Invalid("pending wait packet lost its target"))?;
        // Eligible targets never require mutant ownership; this notification
        // is independent of a guest waiting thread's identity.
        if signaled(p, target, 0) {
            validate_consumption(p, target)?;
            p.objects
                .prepare_wait_packet(packet)
                .map_err(SyncError::Invalid)?;
            consume(p, target, 0);
            p.objects.complete_wait_packet(packet);
        }
    }
    Ok(())
}
