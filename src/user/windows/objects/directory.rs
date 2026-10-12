//! NT directory resources share identity without using Win32 Local/Global names.
use super::{Object, Objects};
use crate::user::windows::object_namespace::Directory;
use std::sync::Arc;
impl Objects {
    /// O(O + H) time and O(1) additional storage for O objects and H handles.
    pub(crate) fn open_directory(
        &mut self,
        directory: Arc<Directory>,
        inherit: bool,
        grant: u32,
    ) -> Option<u32> {
        let existing = self.iter().find_map(|(id, obj)| {
            matches!(obj,Object::Directory(d)if Arc::ptr_eq(d,&directory)).then_some(id)
        });
        let id = match existing {
            Some(id) => id,
            None => self.try_create(Object::Directory(directory))?,
        };
        let handle = self.open_access(id, inherit, grant);
        if handle.is_none() && existing.is_none() {
            self.release(id);
        }
        handle
    }
}
