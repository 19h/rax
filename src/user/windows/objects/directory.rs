//! NT directory resources share identity without using Win32 Local/Global names.
use super::{Object, Objects};
use crate::user::windows::object_namespace::{
    DataSection, Directory, ImageSection, SelectedSection, SymbolicLink,
};
use std::sync::Arc;
impl Objects {
    pub(crate) fn open_selected_section(
        &mut self,
        section: SelectedSection,
        inherit: bool,
        grant: u32,
    ) -> Option<u32> {
        match section {
            SelectedSection::Image(section) => self.open_image_section(section, inherit, grant),
            SelectedSection::Data(section) => self.open_data_section(section, inherit, grant),
        }
    }
    /// Shares selected data identity across independently granted handles.
    pub(crate) fn open_data_section(
        &mut self,
        section: Arc<DataSection>,
        inherit: bool,
        grant: u32,
    ) -> Option<u32> {
        let existing = self.iter().find_map(|(id, obj)| {
            matches!(obj, Object::DataSection(s) if Arc::ptr_eq(s, &section)).then_some(id)
        });
        let id = match existing {
            Some(id) => id,
            None => self.try_create(Object::DataSection(section))?,
        };
        let handle = self.open_access(id, inherit, grant);
        if handle.is_none() && existing.is_none() {
            self.release(id);
        }
        handle
    }
    /// O(C + (H + 1) log(H + 1)) time, O(1) auxiliary scan storage.
    pub(crate) fn open_image_section(
        &mut self,
        section: Arc<ImageSection>,
        inherit: bool,
        grant: u32,
    ) -> Option<u32> {
        let existing = self.iter().find_map(|(id, obj)| {
            matches!(obj,Object::ImageSection(s) if Arc::ptr_eq(s,&section)).then_some(id)
        });
        let id = match existing {
            Some(id) => id,
            None => self.try_create(Object::ImageSection(section))?,
        };
        let handle = self.open_access(id, inherit, grant);
        if handle.is_none() && existing.is_none() {
            self.release(id);
        }
        handle
    }
    /// Expected O(C + (H + 1) log(H + 1)) time for object capacity C and H handles.
    /// Table growth is retained allocation; the scan uses O(1) auxiliary storage.
    pub(crate) fn open_symbolic_link(
        &mut self,
        link: Arc<SymbolicLink>,
        inherit: bool,
        grant: u32,
    ) -> Option<u32> {
        let existing = self.iter().find_map(|(id, obj)| {
            matches!(obj,Object::SymbolicLink(l)if Arc::ptr_eq(l,&link)).then_some(id)
        });
        let id = match existing {
            Some(id) => id,
            None => self.try_create(Object::SymbolicLink(link))?,
        };
        let handle = self.open_access(id, inherit, grant);
        if handle.is_none() && existing.is_none() {
            self.release(id);
        }
        handle
    }
    /// Expected O(C + (H + 1) log(H + 1)) time for object capacity C and H handles.
    /// Table growth is retained allocation; the scan uses O(1) auxiliary storage.
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
