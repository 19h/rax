//! Matched CSR metadata and a private server address space, without host IPC.
use super::{DataSection, LookupError, Namespace, SelectedSection, invalid};
use crate::user::mm::{AddressSpace, SharedObject, SpaceConfig};
use crate::user::windows::arch::WinArch;
use crate::user::windows::memory::{VirtualMemory, VmError, prot};
use crate::user::windows::objects::{LpcPort, Object, Objects};
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Native server pointers remain protocol address values; the selected
/// readonly section supplies their bytes independently at a guest-owned base.
#[derive(Debug, Clone)]
pub(crate) struct CsrMetadata {
    pub(crate) arch: WinArch,
    pub(crate) session: u32,
    pub(crate) server_base: u64,
    pub(crate) statics_offset: u64,
    pub(crate) server_pid: u32,
}

#[derive(Debug)]
pub(crate) struct CsrServer {
    pub(crate) metadata: CsrMetadata,
    section: Arc<DataSection>,
    connections: Arc<AtomicU32>,
}

impl CsrServer {
    pub(crate) fn selected(
        section: Arc<DataSection>,
        metadata: CsrMetadata,
    ) -> io::Result<Arc<Self>> {
        let prefix = section
            .regions
            .first()
            .and_then(|region| region.bytes.as_ref())
            .ok_or_else(|| invalid("CSR metadata has no committed prefix"))?;
        let server_end = metadata.server_base.checked_add(section.size);
        let statics = metadata.statics_offset.checked_add(8);
        if metadata.arch != WinArch::Arm64
            || metadata.session != 0
            || section.size != 1 << 20
            || section.regions.len() != 2
            || section.regions[0].size != 5 * 4096
            || section.regions[1].bytes.is_some()
            || metadata.server_pid == 0
            || metadata.server_pid % 4 != 0
            || metadata.server_base == 0
            || metadata.server_base % 65536 != 0
            || server_end.is_none_or(|end| end > 1 << 47)
            || metadata.statics_offset != 0x750
            || statics.is_none_or(|end| end > prefix.len() as u64)
            || prefix.get(40..48) != Some(metadata.server_base.to_le_bytes().as_slice())
            || prefix.get(48..56) != Some(metadata.server_base.to_le_bytes().as_slice())
        {
            return Err(invalid(
                "CSR metadata differs from selected ARM64/session0 section profile",
            ));
        }
        Ok(Arc::new(Self {
            metadata,
            section,
            connections: Arc::new(AtomicU32::new(0)),
        }))
    }

    /// A separate modeled server maps the real pagefile backing and selected
    /// readonly section. No address in the host's CSR process is dereferenced.
    /// The server mapping owns a typed peer endpoint until this connection ends.
    pub(crate) fn peer(&self, backing: Arc<SharedObject>, size: u64) -> Result<CsrPeer, VmError> {
        self.connections
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                if count < 32 { Some(count + 1) } else { None }
            })
            .map_err(|_| VmError::NoMemory)?;
        let lease = ConnectionLease(self.connections.clone());
        let space = AddressSpace::new(SpaceConfig {
            va_limit: 1 << 47,
            arena_bytes: 1 << 20,
            reserved_phys: vec![],
        })
        .map_err(|_| VmError::NoMemory)?;
        let mut vm = VirtualMemory::new(space, 65536, 1 << 47);
        vm.map_data_section(
            self.section.clone(),
            self.metadata.server_base,
            0,
            self.section.size,
            prot::READONLY,
        )?;
        let view = vm.map_shared_view(backing, Some(0x1_0000_0000), 0, size)?;
        let base = view.base;
        let mut objects = Objects::default();
        let id = objects
            .try_create(Object::LpcPort(LpcPort::new(Some(view))))
            .ok_or(VmError::NoMemory)?;
        let handle = objects
            .open_access(id, false, 0x001F_0001)
            .ok_or(VmError::NoMemory)?;
        Ok(CsrPeer {
            vm,
            objects,
            handle,
            base,
            _lease: lease,
        })
    }
}

/// The server side of one connection, isolated from the caller address space.
/// Dropping the connection releases its entire private server VM/object table.
pub(crate) struct CsrPeer {
    pub(crate) vm: VirtualMemory,
    objects: Objects,
    handle: u32,
    pub(crate) base: u64,
    _lease: ConnectionLease,
}
struct ConnectionLease(Arc<AtomicU32>);
impl Drop for ConnectionLease {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}
impl std::fmt::Debug for CsrPeer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CsrPeer")
            .field("base", &self.base)
            .field("handle", &self.handle)
            .field("handles", &self.objects.handle_count())
            .field("committed-bytes", &self.vm.committed_bytes())
            .finish()
    }
}

impl Namespace {
    pub(crate) fn select_csr_server(&mut self, server: Arc<CsrServer>) -> io::Result<()> {
        if self.csr_server.is_some() {
            return Err(invalid("CSR server selection is immutable"));
        }
        let name: Vec<_> = "\\Windows\\ApiPort".encode_utf16().collect();
        self.lookup_port(&name)
            .map_err(|_| invalid("selected CSR port entry is absent"))?;
        let name: Vec<_> = "\\Windows\\SharedSection".encode_utf16().collect();
        match self.lookup_installed_section(None, &name, 0, false) {
            Ok(SelectedSection::Data(section)) if Arc::ptr_eq(&section, &server.section) => {}
            _ => {
                return Err(invalid(
                    "CSR server and namespace section identities differ",
                ));
            }
        }
        self.csr_server = Some(server);
        Ok(())
    }

    pub(crate) fn csr_server(&self, name: &[u16]) -> Result<Arc<CsrServer>, LookupError> {
        self.lookup_port(name)?;
        if !name.iter().copied().eq("\\Windows\\ApiPort".encode_utf16()) {
            return Err(LookupError::Unselected);
        }
        self.csr_server.clone().ok_or(LookupError::Unselected)
    }
}
