//! Private NT names share object lifetime and identity with all guest handles.
use super::{ObjId, Object, Objects};
use crate::user::windows::object_namespace::NativeName;

impl Objects {
    /// O(C * U) time and O(1) scan storage for object capacity C and name units U.
    /// Newest insertion wins an explicitly insensitive lookup when distinct
    /// exact-case names coexist, matching the retained native naming sequence.
    pub(crate) fn by_native_name(&self, name: &NativeName) -> Option<ObjId> {
        self.objs
            .iter()
            .filter_map(|(&id, entry)| {
                entry
                    .native_name
                    .as_ref()
                    .filter(|stored| name.matches(stored))
                    .map(|_| id)
            })
            .max()
    }

    pub(crate) fn try_create_native_named(
        &mut self,
        name: NativeName,
        object: Object,
    ) -> Option<(ObjId, bool)> {
        if let Some(id) = self.by_native_name(&name) {
            return Some((id, true));
        }
        let id = self.try_create(object)?;
        self.objs.get_mut(&id)?.native_name = Some(name);
        Some((id, false))
    }
}
