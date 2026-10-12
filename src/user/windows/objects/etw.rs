//! Once-only process ETW notification Event ownership.
use super::{ObjId, Object, Objects};

impl Objects {
    /// Pins the first Event without changing its type or signal state.
    /// The caller validates handle/type/access before the busy check.
    /// Expected O(1) time and O(1) retained state, independent of WNF.
    pub(crate) fn register_etw_event(&mut self, id: ObjId) -> Result<bool, &'static str> {
        if self.etw_event.is_some() {
            return Ok(false);
        }
        if !matches!(self.obj(id), Some(Object::Event { .. })) {
            return Err("ETW notification object is not an Event");
        }
        self.retain_many(&[id])?;
        self.etw_event = Some(id);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_etw_notification_reference_overflow_and_drain_are_transactional() {
        let mut objects = Objects::default();
        let id = objects.create(Object::Event {
            manual: true,
            signaled: 1,
        });
        objects.objs.get_mut(&id).unwrap().refs = u32::MAX;
        assert!(objects.register_etw_event(id).is_err());
        assert_eq!(objects.etw_event, None);
        assert_eq!(objects.objs[&id].refs, u32::MAX);
        objects.objs.get_mut(&id).unwrap().refs = 0;
        assert_eq!(objects.register_etw_event(id), Ok(true));
        assert_eq!(objects.objs[&id].refs, 1);
        assert_eq!(objects.register_etw_event(id), Ok(false));
        assert_eq!(objects.objs[&id].refs, 1);
        assert_eq!(objects.register_wnf_event(id), Ok(true));
        assert_eq!(objects.objs[&id].refs, 2);
        assert!(matches!(
            objects.obj(id),
            Some(Object::Event {
                manual: true,
                signaled: 1
            })
        ));
        objects.drain().for_each(drop);
        assert_eq!((objects.etw_event, objects.wnf_event), (None, None));
        assert_eq!(objects.iter().count(), 0);
    }
}
