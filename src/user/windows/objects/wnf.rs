//! Process notification ownership; delivery requires separate WNF state services.
use super::{ObjId, Object, Objects};

impl Objects {
    /// Atomically pins the first registration. The service checks type/grants
    /// before this busy check. Expected O(1) time and O(1) extra space.
    pub(crate) fn register_wnf_event(&mut self, id: ObjId) -> Result<bool, &'static str> {
        if self.wnf_event.is_some() {
            return Ok(false);
        }
        if !matches!(self.obj(id), Some(Object::Event { .. })) {
            return Err("WNF notification object is not an Event");
        }
        self.retain_many(&[id])?;
        self.wnf_event = Some(id);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_wnf_process_event_reference_overflow_is_transactional() {
        let mut objects = Objects::default();
        let id = objects.create(Object::Event {
            manual: true,
            signaled: 0,
        });
        objects.objs.get_mut(&id).unwrap().refs = u32::MAX;
        assert!(objects.register_wnf_event(id).is_err());
        assert_eq!(objects.wnf_event, None);
        assert_eq!(objects.objs[&id].refs, u32::MAX);
        objects.objs.get_mut(&id).unwrap().refs = 0;
        assert_eq!(objects.register_wnf_event(id), Ok(true));
        assert_eq!(objects.wnf_event, Some(id));
        assert_eq!(objects.objs[&id].refs, 1);
        assert_eq!(objects.register_wnf_event(id), Ok(false));
        assert_eq!(objects.objs[&id].refs, 1);
        objects.drain().for_each(drop);
        assert_eq!(objects.wnf_event, None);
    }
}
