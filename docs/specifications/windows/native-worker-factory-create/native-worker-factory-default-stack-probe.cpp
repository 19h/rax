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
using Create = LONG(NTAPI *)(PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES, HANDLE, HANDLE, PVOID, PVOID, ULONG, SIZE_T, SIZE_T);
using CreatePort = LONG(NTAPI *)(PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES, ULONG);
using Query = LONG(NTAPI *)(HANDLE, ULONG, PVOID, ULONG, PULONG);
using Close = LONG(NTAPI *)(HANDLE);
static Create create_factory;
static CreatePort create_port;
static Query query_factory, query_object;
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
static LONG invoke(HANDLE *out, ULONG access, OBJECT_ATTRIBUTES *attrs, ULONG count, ULONG *exception, HANDLE port, HANDLE process, uintptr_t start, uintptr_t parameter, SIZE_T reserve, SIZE_T commit) {
    LONG status=static_cast<LONG>(0xDEADBEEF); *exception=0;
    __try { status=create_factory(out,access,attrs,port,process,reinterpret_cast<PVOID>(start),reinterpret_cast<PVOID>(parameter),count,reserve,commit); }
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
static void run(const char *label,ULONG count,ULONG access,unsigned mode=0,unsigned offset=0,ULONG flags=0,ULONG length=sizeof(OBJECT_ATTRIBUTES), ULONG port_access=0xF003F, unsigned port_kind=0, unsigned process_kind=0, ULONG process_access=0x1FFFFF, uintptr_t start=0x12345678, uintptr_t parameter=0x87654321, SIZE_T reserve=0, SIZE_T commit=0) {
    DWORD fixture_before=0,fixture_after=0;GetProcessHandleCount(GetCurrentProcess(),&fixture_before);
    HANDLE port=nullptr, process=GetCurrentProcess(), fixture_port=nullptr, fixture_process=nullptr, event=nullptr;
    if(create_port(&fixture_port,port_access,nullptr,4)<0)ExitProcess(12);
    port=fixture_port;
    if(port_kind==1)port=nullptr;
    if(port_kind==2)port=reinterpret_cast<HANDLE>(0x1234);
    if(port_kind==3)port=GetCurrentProcess();
    if(port_kind==4||process_kind==5){event=CreateEventW(nullptr,TRUE,FALSE,nullptr);if(!event)ExitProcess(13);if(port_kind==4)port=event;}
    if(port_kind==5)port=reinterpret_cast<HANDLE>(reinterpret_cast<uintptr_t>(port)|1);
    if(process_kind==1||process_kind==7){if(!DuplicateHandle(GetCurrentProcess(),GetCurrentProcess(),GetCurrentProcess(),&fixture_process,process_access,FALSE,0))ExitProcess(14);process=fixture_process;if(process_kind==7)process=reinterpret_cast<HANDLE>(reinterpret_cast<uintptr_t>(process)|1);}
    if(process_kind==2)process=nullptr;
    if(process_kind==3)process=reinterpret_cast<HANDLE>(0x1234);
    if(process_kind==4)process=GetCurrentThread();
    if(process_kind==5)process=event;
    if(process_kind==6)process=fixture_port;
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
        ULONG exception=0;auto status=invoke(op,access,oa,count,&exception,port,process,start,parameter,reserve,commit);
        HANDLE handle=nullptr;auto handle_bytes=op?safe_read(op,&handle,sizeof handle):0;
        unsigned char basic[128]{}, worker[128];std::memset(worker,0xA5,sizeof worker);ULONG returned=0xA5A5A5A5;
        LONG qs=static_cast<LONG>(0xDEADBEEF),bs=qs,cs=qs;DWORD handle_flags=0;
        if(status>=0 && !exception) {
            if(handle_bytes!=sizeof handle||!handle)ExitProcess(7);
            bs=query_object(handle,0,basic,sizeof(PUBLIC_OBJECT_BASIC_INFORMATION),nullptr);
            qs=query_factory(handle,7,worker,sizeof(void *)==8?120:96,&returned);
            if(!GetHandleInformation(handle,&handle_flags))ExitProcess(8);
            cs=close_object(handle);if(cs<0)ExitProcess(9);
        }
        if(!GetProcessHandleCount(GetCurrentProcess(),&after))ExitProcess(10);
        ULONG grant=0;std::memcpy(&grant,basic+4,sizeof grant);
        std::printf("case=%lu label=%s width=%zu count=%08lX access=%08lX mode=%u offset=%u flags=%08lX attrs-length=%lu repeat=%u status=%08lX exception=%08lX handle-bytes=%zu handle=%llX query-object=%08lX grant=%08lX query=%08lX returned=%08lX handle-flags=%lu close=%08lX handles-before=%lu handles-after=%lu output-guard=%lu attrs-guard=%lu",ordinal++,label,sizeof(void *),count,access,mode,offset,flags,length,repeat,static_cast<ULONG>(status),exception,handle_bytes,static_cast<unsigned long long>(reinterpret_cast<uintptr_t>(handle)),static_cast<ULONG>(bs),grant,static_cast<ULONG>(qs),returned,handle_flags,static_cast<ULONG>(cs),before,after,guard(output.p),guard(attributes.p));
        std::printf(" port-access=%08lX port-kind=%u process-kind=%u process-access=%08lX start=%llX parameter=%llX reserve=%llX commit=%llX worker=",port_access,port_kind,process_kind,process_access,static_cast<unsigned long long>(start),static_cast<unsigned long long>(parameter),static_cast<unsigned long long>(reserve),static_cast<unsigned long long>(commit));
        for(unsigned i=0;i<sizeof worker;++i)std::printf("%02X",worker[i]);
        hex("output",op,96);hex("output-backing",backing,96);hex("attributes",oa,sizeof(OBJECT_ATTRIBUTES));std::puts("");
    }
    if(close_object(fixture_port)<0)ExitProcess(15);if(fixture_process&&close_object(fixture_process)<0)ExitProcess(16);if(event&&close_object(event)<0)ExitProcess(17);
    GetProcessHandleCount(GetCurrentProcess(),&fixture_after);if(fixture_before!=fixture_after)ExitProcess(18);
}
int main() {
    SYSTEM_INFO info{};GetSystemInfo(&info);page=info.dwPageSize;
    auto module=GetModuleHandleW(L"ntdll.dll");create_factory=reinterpret_cast<Create>(GetProcAddress(module,"NtCreateWorkerFactory"));create_port=reinterpret_cast<CreatePort>(GetProcAddress(module,"NtCreateIoCompletion"));query_factory=reinterpret_cast<Query>(GetProcAddress(module,"NtQueryInformationWorkerFactory"));query_object=reinterpret_cast<Query>(GetProcAddress(module,"NtQueryObject"));close_object=reinterpret_cast<Close>(GetProcAddress(module,"NtClose"));
    if(!create_factory||!create_port||!query_factory||!query_object||!close_object)return 11;
    std::printf("profile width=%zu page=%lu processors=%lu attrs-size=%zu\n",sizeof(void *),page,info.dwNumberOfProcessors,sizeof(OBJECT_ATTRIBUTES));
    run("default-stack",512,0xF00FF,0,0,0,sizeof(OBJECT_ATTRIBUTES),0x1F0003,0,0,0x1FFFFF,0x12345678,0x87654321,0,0);
    std::printf("complete cases=%lu\n",ordinal);return 0;
}
