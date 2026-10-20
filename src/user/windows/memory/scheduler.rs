//! Fixed scheduler views with identity-checked retirement, including ABA reuse.
use super::*;

#[derive(Debug, Clone)]
pub(crate) struct SchedulerView {
    pub(crate) base: u64,
    token: Arc<str>,
}
impl VirtualMemory {
    pub(crate) fn create_scheduler_view(&mut self) -> Result<SchedulerView, VmError> {
        let token: Arc<str> = Arc::from("[SchedulerSharedData]");
        let base = self.reserve(
            None,
            8192,
            prot::READWRITE,
            AllocKind::Mapped,
            false,
            Some(token.clone()),
        )?;
        if let Err(error) = self.commit(base, PAGE_SIZE, prot::READWRITE) {
            let _ = self.release(base);
            return Err(error);
        }
        self.pagefile_views.insert(base);
        Ok(SchedulerView { base, token })
    }
    fn owns_scheduler_view(&self, view: &SchedulerView) -> bool {
        self.allocation(view.base).is_some_and(|a| {
            a.base == view.base
                && a.kind == AllocKind::Mapped
                && a.name
                    .as_ref()
                    .is_some_and(|name| Arc::ptr_eq(name, &view.token))
        })
    }
    pub(crate) fn prepare_scheduler_slot(
        &mut self,
        view: &SchedulerView,
        index: u32,
    ) -> Result<(), VmError> {
        if index >= 128 {
            return Err(VmError::InvalidParameter);
        }
        // Native explicit section unmap does not invalidate the owner's cached
        // returned address. Never commit or clear a replacement allocation.
        if !self.owns_scheduler_view(view) {
            return Ok(());
        }
        let slot = view.base + u64::from(index) * 64;
        let page = slot & !(PAGE_SIZE - 1);
        if self.query(page).is_some_and(|q| q.state == mem::RESERVE) {
            self.commit(page, PAGE_SIZE, prot::READWRITE)?;
        }
        // Kernel-owned initialization does not consume guest guard pages or
        // change guest protection. A decommitted/replaced page is left intact.
        if self.query(slot).is_some_and(|q| q.state == mem::COMMIT) {
            self.space
                .write_raw(slot, &[0; 64])
                .map_err(|_| VmError::NotCommitted)?;
        }
        Ok(())
    }
    pub(crate) fn retire_scheduler_view(&mut self, view: &SchedulerView) -> Result<(), VmError> {
        if self.owns_scheduler_view(view) {
            self.release(view.base)?;
        }
        Ok(())
    }
}
