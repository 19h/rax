#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <winternl.h>
#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
using Create = LONG(NTAPI *)(PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES, ULONG);
using Query = LONG(NTAPI *)(HANDLE, ULONG, PVOID, ULONG, PULONG);
using Close = LONG(NTAPI *)(HANDLE);
static Create create_port;
static Query query_port, query_object;
static Close close_object;
static ULONG page, ordinal;
struct Region {
    unsigned char *p;
    Region() : p(static_cast<unsigned char *>(VirtualAlloc(nullptr, 8*page, MEM_RESERVE|MEM_COMMIT, PAGE_READWRITE))) {
        if (!p) ExitProcess(2);
        std::memset(p, 0xA5, 8*page);
    }
    ~Region() { if (!VirtualFree(p, 0, MEM_RELEASE)) ExitProcess(3); }
    void protect(unsigned offset, DWORD value) { DWORD old; if (!VirtualProtect(p+offset,page,value,&old)) ExitProcess(4); }
};
static LONG invoke(HANDLE *out, ULONG access, OBJECT_ATTRIBUTES *attrs, ULONG count, ULONG *exception) {
    LONG status=static_cast<LONG>(0xDEADBEEF); *exception=0;
    __try { status=create_port(out,access,attrs,count); }
    __except(EXCEPTION_EXECUTE_HANDLER) { *exception=GetExceptionCode(); }
    return status;
}
static SIZE_T safe_read(const void *p, void *out, SIZE_T bytes) {
    MEMORY_BASIC_INFORMATION info; SIZE_T got=0;
    if(!VirtualQuery(p,&info,sizeof info)||info.State!=MEM_COMMIT||(info.Protect&(PAGE_NOACCESS|PAGE_GUARD)))return 0;
    auto available=static_cast<const unsigned char *>(info.BaseAddress)+info.RegionSize-static_cast<const unsigned char *>(p);
    if(!ReadProcessMemory(GetCurrentProcess(),p,out,std::min(bytes,static_cast<SIZE_T>(available)),&got))ExitProcess(5);
    return got;
}
static void hex(const char *name,const void *p,unsigned bytes) {
    unsigned char data[128]; auto got=safe_read(p,data,std::min<unsigned>(bytes,sizeof data));
    std::printf(" %s-captured=%zu %s=",name,got,name);
    for(SIZE_T i=0;i<got;++i)std::printf("%02X",data[i]);
}
static ULONG guard(const void *p) { MEMORY_BASIC_INFORMATION info{}; return VirtualQuery(p,&info,sizeof info)?info.Protect&PAGE_GUARD:0; }
static void run(const char *label,ULONG count,ULONG access,unsigned mode=0,unsigned offset=0,ULONG flags=0,ULONG length=sizeof(OBJECT_ATTRIBUTES)) {
    Region output, attributes, extra;
    auto *op=reinterpret_cast<HANDLE *>(output.p+128);
    auto *oa=reinterpret_cast<OBJECT_ATTRIBUTES *>(attributes.p+128);
    OBJECT_ATTRIBUTES value{};value.Length=length;value.Attributes=flags;
    std::memcpy(oa,&value,sizeof value);bool use_attrs=mode>=5;
    switch(mode) {
    case 1:op=nullptr;break;
    case 2:op=reinterpret_cast<HANDLE *>(output.p+128+offset);break;
    case 3:output.protect(0,PAGE_READONLY);break;
    case 4:output.protect(0,PAGE_READWRITE|PAGE_GUARD);break;
    case 5:break;
    case 6:oa=reinterpret_cast<OBJECT_ATTRIBUTES *>(1);break;
    case 7:oa=reinterpret_cast<OBJECT_ATTRIBUTES *>(attributes.p+128+offset);std::memcpy(oa,&value,sizeof value);break;
    case 8:attributes.protect(0,PAGE_NOACCESS);break;
    case 9:attributes.protect(0,PAGE_READWRITE|PAGE_GUARD);break;
    case 10:oa->RootDirectory=reinterpret_cast<HANDLE>(static_cast<uintptr_t>(offset));break;
    case 11:oa->ObjectName=reinterpret_cast<UNICODE_STRING *>(1);break;
    case 12:oa->SecurityDescriptor=reinterpret_cast<void *>(1);break;
    case 13:oa->SecurityQualityOfService=reinterpret_cast<void *>(1);break;
    case 14:op=nullptr;oa=reinterpret_cast<OBJECT_ATTRIBUTES *>(1);break;
    case 15:op=nullptr;break;
    case 16:output.protect(0,PAGE_READONLY);break;
    case 17:output.protect(0,PAGE_READWRITE|PAGE_GUARD);break;
    case 18:attributes.protect(0,PAGE_NOACCESS);op=nullptr;break;
    case 19:attributes.protect(0,PAGE_READWRITE|PAGE_GUARD);op=nullptr;break;
    case 20:op=reinterpret_cast<HANDLE *>(oa);break;
    case 21:op=reinterpret_cast<HANDLE *>(reinterpret_cast<unsigned char *>(oa)+offset);break;
    case 22:oa=reinterpret_cast<OBJECT_ATTRIBUTES *>(attributes.p+page-offset);std::memcpy(oa,&value,sizeof value);attributes.protect(page,PAGE_NOACCESS);break;
    case 23:op=reinterpret_cast<HANDLE *>(output.p+page-offset);output.protect(page,PAGE_NOACCESS);use_attrs=false;break;
    case 24:oa->ObjectName=reinterpret_cast<UNICODE_STRING *>(extra.p+128);std::memset(extra.p+128,0,sizeof(UNICODE_STRING));break;
    case 25:oa->SecurityQualityOfService=extra.p+128;std::memset(extra.p+128,0,16);*reinterpret_cast<ULONG *>(extra.p+128)=12;break;
    case 26:oa->SecurityDescriptor=extra.p+128;std::memset(extra.p+128,0,64);extra.p[128]=1;break;
    case 27:oa=reinterpret_cast<OBJECT_ATTRIBUTES *>(attributes.p+128+offset);std::memcpy(oa,&value,sizeof value);op=nullptr;break;
    }
    if(!use_attrs)oa=nullptr;
    auto *backing=mode==20||mode==21?attributes.p+128:output.p+128;
    unsigned repeats=(mode==4||mode==9||mode==17||mode==19)?2:1;
    for(unsigned repeat=0;repeat<repeats;++repeat) {
        DWORD before=0,after=0;if(!GetProcessHandleCount(GetCurrentProcess(),&before))ExitProcess(6);
        ULONG exception=0;auto status=invoke(op,access,oa,count,&exception);
        HANDLE handle=nullptr;auto handle_bytes=op?safe_read(op,&handle,sizeof handle):0;
        unsigned char basic[128]{};LONG depth=-1;ULONG returned=0xA5A5A5A5;
        LONG qs=static_cast<LONG>(0xDEADBEEF),bs=qs,cs=qs;DWORD handle_flags=0;
        if(status>=0 && !exception) {
            if(handle_bytes!=sizeof handle||!handle)ExitProcess(7);
            bs=query_object(handle,0,basic,sizeof(PUBLIC_OBJECT_BASIC_INFORMATION),nullptr);
            qs=query_port(handle,0,&depth,sizeof depth,&returned);
            if(!GetHandleInformation(handle,&handle_flags))ExitProcess(8);
            cs=close_object(handle);if(cs<0)ExitProcess(9);
        }
        if(!GetProcessHandleCount(GetCurrentProcess(),&after))ExitProcess(10);
        ULONG grant=0;std::memcpy(&grant,basic+4,sizeof grant);
        std::printf("case=%lu label=%s width=%zu count=%08lX access=%08lX mode=%u offset=%u flags=%08lX attrs-length=%lu repeat=%u status=%08lX exception=%08lX handle-bytes=%zu handle=%llX query-object=%08lX grant=%08lX query=%08lX depth=%ld returned=%08lX handle-flags=%lu close=%08lX handles-before=%lu handles-after=%lu output-guard=%lu attrs-guard=%lu",ordinal++,label,sizeof(void *),count,access,mode,offset,flags,length,repeat,static_cast<ULONG>(status),exception,handle_bytes,static_cast<unsigned long long>(reinterpret_cast<uintptr_t>(handle)),static_cast<ULONG>(bs),grant,static_cast<ULONG>(qs),depth,returned,handle_flags,static_cast<ULONG>(cs),before,after,guard(output.p),guard(attributes.p));
        hex("output",op,96);hex("output-backing",backing,96);hex("attributes",oa,sizeof(OBJECT_ATTRIBUTES));std::puts("");
    }
}
int main() {
    SYSTEM_INFO info{};GetSystemInfo(&info);page=info.dwPageSize;
    auto module=GetModuleHandleW(L"ntdll.dll");create_port=reinterpret_cast<Create>(GetProcAddress(module,"NtCreateIoCompletion"));query_port=reinterpret_cast<Query>(GetProcAddress(module,"NtQueryIoCompletion"));query_object=reinterpret_cast<Query>(GetProcAddress(module,"NtQueryObject"));close_object=reinterpret_cast<Close>(GetProcAddress(module,"NtClose"));
    if(!create_port||!query_port||!query_object||!close_object)return 11;
    std::printf("profile width=%zu page=%lu processors=%lu attrs-size=%zu\n",sizeof(void *),page,info.dwNumberOfProcessors,sizeof(OBJECT_ATTRIBUTES));
    for(unsigned root:{0u,4u,0x1234u,0x7FFFFFFFu,0x80000000u,0xFFFFFFFEu,0xFFFFFFFFu})
        for(ULONG flags:{0ul,2ul,0x80000000ul})run("root-directory",4,0x1F0003,10,root,flags);
    std::printf("complete cases=%lu\n",ordinal);
    return 0;
}
