//! Communication endpoint views follow the final local handle, not Arc Drop.
use super::{ObjId, Object, Objects};
use crate::user::windows::memory::{SharedViewOwner, VirtualMemory, VmError};

/// Typed process-local LPC endpoint. This object never owns a host port handle
/// and alone does not establish a connection or admit message services.
#[derive(Debug)]
pub struct LpcPort {
    view: Option<SharedViewOwner>,
    peer: Option<Box<crate::user::windows::object_namespace::csr::CsrPeer>>,
}

impl LpcPort {
    pub(crate) fn new(view: Option<SharedViewOwner>) -> Self {
        Self { view, peer: None }
    }

    pub(crate) fn csr(
        view: SharedViewOwner,
        peer: crate::user::windows::object_namespace::csr::CsrPeer,
    ) -> Self {
        Self {
            view: Some(view),
            peer: Some(Box::new(peer)),
        }
    }

    pub(crate) fn peer(&self) -> Option<&crate::user::windows::object_namespace::csr::CsrPeer> {
        self.peer.as_deref()
    }

    pub(crate) fn peer_mut(
        &mut self,
    ) -> Option<&mut crate::user::windows::object_namespace::csr::CsrPeer> {
        self.peer.as_deref_mut()
    }

    pub(crate) fn view(&self) -> Option<&SharedViewOwner> {
        self.view.as_ref()
    }

    pub(super) fn take_view(&mut self) -> Option<SharedViewOwner> {
        self.view.take()
    }
}

impl Objects {
    /// Closing a duplicate preserves the local view. The last handle retires
    /// it even when an internal wait/reference still retains the object.
    pub(super) fn retire_lpc_handle(&mut self, id: ObjId) {
        if let Some(Object::LpcPort(port)) = self.obj_mut(id)
            && let Some(view) = port.take_view()
        {
            self.retired_lpc_views.push(view);
        }
    }

    /// Covers rollback of an unpublished endpoint as well as final release.
    pub(super) fn retire_lpc_object(&mut self, object: &mut Object) {
        if let Object::LpcPort(port) = object
            && let Some(view) = port.take_view()
        {
            self.retired_lpc_views.push(view);
        }
    }

    /// Native/HLE completion and process drain use the same retirement queue.
    /// A failed release stays queued; an already unmapped/replaced view is a
    /// successful no-op under its allocation generation token.
    pub(crate) fn reclaim_lpc_views(&mut self, vm: &mut VirtualMemory) -> Result<(), VmError> {
        while let Some(view) = self.retired_lpc_views.last() {
            vm.retire_shared_view(view)?;
            self.retired_lpc_views.pop();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
