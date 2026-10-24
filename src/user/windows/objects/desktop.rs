//! The selected process desktop and its borrowed, intrinsically attached handle.
use super::{ObjId, Object, Objects};
use std::io;
use std::sync::Arc;

/// Immutable desktop metadata; never a host handle or host object pointer.
#[derive(Debug)]
pub struct Desktop {
    pub(crate) name: Arc<[u16]>,
    pub(crate) access: u32,
    pub(crate) flags: u32,
}

impl Desktop {
    /// Native UTF-16 desktop name, without its terminating zero.
    pub fn name(&self) -> &[u16] {
        &self.name
    }

    pub(crate) fn selected(name: Vec<u16>, access: u32, flags: u32) -> io::Result<Arc<Self>> {
        if name.is_empty() || name.len() > 1023 || name.contains(&0) || flags & !3 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid selected desktop name",
            ));
        }
        Ok(Arc::new(Self {
            name: name.into(),
            access,
            flags,
        }))
    }
}

impl Objects {
    /// Establish the process-owned borrowed handle before guest execution.
    /// Repeated queries do not allocate another handle or change its flags.
    pub(crate) fn assign_desktop(&mut self, desktop: Arc<Desktop>) -> Option<u32> {
        if let Some((id, handle)) = self.desktop {
            return (self.id(u64::from(handle)) == Some(id)
                && matches!(self.obj(id), Some(Object::Desktop(selected)) if Arc::ptr_eq(selected, &desktop)))
                .then_some(handle);
        }
        let access = desktop.access;
        let flags = desktop.flags;
        let id = self.try_create(Object::Desktop(desktop))?;
        let Some(handle) = self.open_access(id, flags & 1 != 0, access) else {
            self.release(id);
            return None;
        };
        self.set_flags(u64::from(handle), 3, flags);
        self.desktop = Some((id, handle));
        Some(handle)
    }

    pub(crate) fn desktop_handle(&self, desktop: &Arc<Desktop>) -> Option<u32> {
        let (id, handle) = self.desktop?;
        (self.id(u64::from(handle)) == Some(id)
            && matches!(self.obj(id), Some(Object::Desktop(selected)) if Arc::ptr_eq(selected, desktop)))
            .then_some(handle)
    }

    /// Attachment denies close independently of HANDLE_FLAG_PROTECT_FROM_CLOSE.
    pub(crate) fn assigned_desktop_handle(&self, handle: u64) -> bool {
        self.desktop.is_some_and(|(id, assigned)| {
            Self::key(handle) == Some(assigned) && self.id(handle) == Some(id)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_attachment_is_distinct_from_handle_flags_and_duplicate_lifetime() {
        let selected = Desktop::selected("Default".encode_utf16().collect(), 0xF00CF, 0).unwrap();
        let mut objects = Objects::default();
        let handle = objects.assign_desktop(selected.clone()).unwrap();
        let id = objects.id(u64::from(handle)).unwrap();
        assert_eq!(objects.assign_desktop(selected.clone()), Some(handle));
        assert_eq!(objects.handle_count(), 1);
        assert_eq!(objects.access(u64::from(handle)), Some(0xF00CF));
        assert_eq!(objects.flags(u64::from(handle)), Some(0));
        for tag in 0..4 {
            assert!(objects.assigned_desktop_handle(u64::from(handle | tag)));
            assert!(objects.close(u64::from(handle | tag)).is_err());
        }
        assert!(objects.set_flags(u64::from(handle), 3, 1));
        assert_eq!(objects.flags(u64::from(handle)), Some(1));
        assert!(objects.close(u64::from(handle)).is_err());
        let copy = objects.duplicate(u64::from(handle), false).unwrap();
        assert_eq!(objects.id(u64::from(copy)), Some(id));
        assert!(!objects.assigned_desktop_handle(u64::from(copy)));
        assert!(objects.close(u64::from(copy)).unwrap().is_none());
        let other = Desktop::selected("Default".encode_utf16().collect(), 0xF00CF, 0).unwrap();
        assert_eq!(objects.assign_desktop(other), None);
        assert_eq!(objects.desktop_handle(&selected), Some(handle));
        assert_eq!(objects.drain().count(), 1);
        assert_eq!(objects.desktop, None);
        let renewed = objects.assign_desktop(selected).unwrap();
        assert_ne!(objects.id(u64::from(renewed)), Some(id));
    }

    #[test]
    fn desktop_identifier_exhaustion_does_not_publish_an_object_or_borrowed_handle() {
        let mut objects = Objects::default();
        objects.next = ObjId::MAX;
        let selected = Desktop::selected("Default".encode_utf16().collect(), 1, 0).unwrap();
        assert_eq!(objects.assign_desktop(selected), None);
        assert_eq!(objects.handle_count(), 0);
        assert_eq!(objects.iter().count(), 0);
        assert_eq!(objects.desktop, None);
    }
}
