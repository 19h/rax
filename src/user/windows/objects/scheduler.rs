//! Captured build29683 process-local scheduler slots and their references.
use super::{ObjId, Object, Objects};
use crate::user::windows::memory::{SchedulerView, VirtualMemory};
use crate::user::windows::nt::status::*;

/// A fixed 128-slot owner. Mapping publication is lazy; handles alone do not
/// allocate a view. One binding independently pins the owner until release.
#[derive(Debug, Default)]
pub struct SchedulerData {
    pub(super) view: Option<SchedulerView>,
    used: u128,
}
impl Objects {
    /// The kernel binding, independent of the guest-writable TEB and fibers.
    pub(crate) fn scheduler_slot(&self, tid: u32) -> u64 {
        self.scheduler_bindings
            .get(&tid)
            .and_then(|&(id, index)| match self.obj(id) {
                Some(Object::SchedulerSharedData { data, .. }) => data
                    .view
                    .as_ref()
                    .map(|view| view.base + u64::from(index) * 64),
                _ => None,
            })
            .unwrap_or(0)
    }
    /// Assigns the first free 64-byte slot: O(log T) binding lookup and O(1)
    /// bitmap selection for T threads. Existing-view VM lookup is O(log A);
    /// initial allocation also performs the allocator's worst-case linear
    /// search across A allocations.
    pub(crate) fn assign_scheduler_slot(
        &mut self,
        vm: &mut VirtualMemory,
        id: ObjId,
        tid: u32,
    ) -> Result<u64, u32> {
        if self.scheduler_slot(tid) != 0 {
            return Err(STATUS_NOT_SUPPORTED);
        }
        let index = match self.obj(id) {
            Some(Object::SchedulerSharedData { data, .. }) => (!data.used).trailing_zeros(),
            _ => return Err(STATUS_OBJECT_TYPE_MISMATCH),
        };
        if index == 128 {
            return Err(STATUS_INSUFFICIENT_RESOURCES);
        }
        self.retain_many(&[id])
            .map_err(|_| STATUS_INSUFFICIENT_RESOURCES)?;
        let result = (|| {
            let Some(Object::SchedulerSharedData { data, .. }) = self.obj_mut(id) else {
                unreachable!("retained scheduler owner");
            };
            if data.view.is_none() {
                data.view = Some(vm.create_scheduler_view().map_err(|e| e.status())?);
            }
            let view = data.view.as_ref().unwrap();
            vm.prepare_scheduler_slot(view, index)
                .map_err(|e| e.status())?;
            data.used |= 1u128 << index;
            Ok(view.base + u64::from(index) * 64)
        })();
        match result {
            Ok(slot) => {
                self.scheduler_bindings.insert(tid, (id, index));
                Ok(slot)
            }
            Err(status) => {
                self.release(id);
                Err(status)
            }
        }
    }
    /// Free ignores caller-provided owner/slot fields. Contents persist until
    /// reassignment; releasing the final binding may retire the complete view.
    pub(crate) fn free_scheduler_slot(&mut self, tid: u32) {
        if let Some((id, index)) = self.scheduler_bindings.remove(&tid) {
            if let Some(Object::SchedulerSharedData { data, .. }) = self.obj_mut(id) {
                data.used &= !(1u128 << index);
            }
            self.release(id);
        }
    }
    pub(super) fn retire_scheduler_object(&mut self, object: &Object) {
        if let Object::SchedulerSharedData { data, .. } = object
            && let Some(view) = &data.view
        {
            self.retired_scheduler_views.push(view.clone());
        }
    }
    /// Reclaim after every HLE/native service boundary, thread destruction and
    /// process drain. This includes NtClose and all close-source duplication.
    pub(crate) fn reclaim_scheduler_views(
        &mut self,
        vm: &mut VirtualMemory,
    ) -> Result<(), super::super::memory::VmError> {
        while let Some(view) = self.retired_scheduler_views.last() {
            vm.retire_scheduler_view(view)?;
            self.retired_scheduler_views.pop();
        }
        Ok(())
    }
}
